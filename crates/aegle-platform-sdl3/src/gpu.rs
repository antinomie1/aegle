//! Owned leases on a window's native handles for graphics APIs.
//!
//! SDL3 publishes the handles of the backend it chose as window properties: a
//! `wl_display`/`wl_surface` pair on Wayland, a `Display`/`Window` pair on X11.
//! The lease keeps the SDL window alive, so a presenter may hold these pointers
//! until it is dropped.
#![allow(unsafe_code)]

use std::{ffi::c_void, ptr::NonNull, rc::Rc};

use aegle_platform::{Error, WindowId};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle, XlibDisplayHandle,
    XlibWindowHandle,
};

use crate::{Sdl3, window::NativeWindow};

/// What the window's handles point into.
enum Native {
    Wayland {
        display: NonNull<c_void>,
        surface: NonNull<c_void>,
    },
    Xlib {
        display: NonNull<c_void>,
        window: u64,
        screen: i32,
    },
}

/// Owned UI-thread lease on one window's native handles.
///
/// Removing its window from [`Sdl3`] stops input and presentation for it; the
/// native window stays alive until every lease is dropped, which keeps these
/// handles valid throughout a graphics surface's destruction. Drop presenters
/// before the platform, because SDL shuts its video driver down with it.
pub struct WindowSurface {
    native: Native,
    /// Keeps the SDL window itself alive while handles point into it. Being
    /// non-`Send` also keeps the lease on the thread that owns the platform.
    _window: Rc<NativeWindow>,
}

impl HasWindowHandle for WindowSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = match self.native {
            Native::Wayland { surface, .. } => {
                RawWindowHandle::Wayland(WaylandWindowHandle::new(surface))
            }
            Native::Xlib { window, .. } => RawWindowHandle::Xlib(XlibWindowHandle::new(window)),
        };
        // SAFETY: the lease owns the SDL window, which SDL destroys only after
        // the last lease is dropped; nothing revokes these handles meanwhile.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for WindowSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = match self.native {
            Native::Wayland { display, .. } => {
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display))
            }
            Native::Xlib {
                display, screen, ..
            } => RawDisplayHandle::Xlib(XlibDisplayHandle::new(Some(display), screen)),
        };
        // SAFETY: the leased window keeps the display connection alive.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

impl std::fmt::Debug for WindowSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WindowSurface")
    }
}

impl Sdl3 {
    /// Returns a lease on a window's native handles, failing with
    /// [`Error::Unsupported`] on a video driver whose handles this backend does
    /// not translate.
    pub(crate) fn window_surface(&self, id: WindowId) -> Result<WindowSurface, Error> {
        let lib = self.lib;
        let window = self.window(id)?;
        // SAFETY: the window is live on this thread; the property group belongs
        // to it and is only read here.
        let properties = unsafe { (lib.get_window_properties)(window.ptr()) };
        if properties == 0 {
            return Err(Error::backend("the window has no properties"));
        }
        // SAFETY: a property read of a valid group; a missing property yields
        // the null default, which becomes `None`.
        let pointer = |name: &std::ffi::CStr| {
            NonNull::new(unsafe {
                (lib.get_pointer_property)(properties, name.as_ptr(), std::ptr::null_mut())
            })
        };
        let number = |name: &std::ffi::CStr| {
            // SAFETY: as above; numbers are stored in the same group.
            unsafe { (lib.get_number_property)(properties, name.as_ptr(), 0) }
        };
        let native = match (
            pointer(c"SDL.window.wayland.display"),
            pointer(c"SDL.window.wayland.surface"),
        ) {
            (Some(display), Some(surface)) => Native::Wayland { display, surface },
            _ => match pointer(c"SDL.window.x11.display") {
                Some(display) => {
                    let window = number(c"SDL.window.x11.window");
                    if window == 0 {
                        return Err(Error::Unsupported("native handles of this video driver"));
                    }
                    Native::Xlib {
                        display,
                        window: window as u64,
                        screen: number(c"SDL.window.x11.screen") as i32,
                    }
                }
                None => return Err(Error::Unsupported("native handles of this video driver")),
            },
        };
        Ok(WindowSurface {
            native,
            _window: window.native.clone(),
        })
    }
}
