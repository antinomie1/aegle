//! Undecorated windows, window areas and the window icon.
//!
//! SDL begins a move or resize only from its hit-test callback, which it
//! calls for a press before reporting it. The callback answers from the
//! window's areas and, for a borderless resizable window, from the
//! contract's resize border, and SDL shows the resize cursors itself.
//! Drivers without hit testing (dummy, offscreen) report
//! [`Error::Unsupported`]: there an undecorated window could not be resized
//! by the user.
#![allow(unsafe_code)]

use crate::{
    sys,
    window::{NativeWindow, Sdl3},
};
use aegle_platform::{
    Error, Icon, PixelSize, Point, Rect, ResizeEdge, Result, WindowArea, WindowId, area_at,
    border_edge, check_areas,
};
use std::{
    ffi::{c_int, c_void},
    rc::Rc,
};

/// SDL's hit-test result that resizes by `edge`.
fn hit_code(edge: ResizeEdge) -> c_int {
    match edge {
        ResizeEdge::TopLeft => sys::HITTEST_RESIZE_TOPLEFT,
        ResizeEdge::Top => sys::HITTEST_RESIZE_TOP,
        ResizeEdge::TopRight => sys::HITTEST_RESIZE_TOPRIGHT,
        ResizeEdge::Right => sys::HITTEST_RESIZE_RIGHT,
        ResizeEdge::BottomRight => sys::HITTEST_RESIZE_BOTTOMRIGHT,
        ResizeEdge::Bottom => sys::HITTEST_RESIZE_BOTTOM,
        ResizeEdge::BottomLeft => sys::HITTEST_RESIZE_BOTTOMLEFT,
        ResizeEdge::Left => sys::HITTEST_RESIZE_LEFT,
    }
}

/// SDL's hit-test callback: what a press at `point`, in window coordinates,
/// does. None acts while full screen, and resizing only on a resizable
/// window in its normal mode, first by the border of a borderless one.
///
/// # Safety
/// `data` is the [`NativeWindow`] the callback was installed with, which
/// owns `window`, and `point` is valid for the call.
unsafe extern "C" fn hit_test(
    window: *mut sys::Window,
    point: *const sys::Point,
    data: *mut c_void,
) -> c_int {
    // SAFETY: per the contract above; only shared references are formed.
    let (native, point) = unsafe { (&*data.cast::<NativeWindow>(), &*point) };
    let lib = native.lib;
    // SAFETY: flag and size queries of the live window SDL is asking about.
    let (flags, size) = unsafe {
        let (mut width, mut height) = (0, 0);
        let flags = (lib.get_window_flags)(window);
        (lib.get_window_size)(window, &mut width, &mut height);
        (flags, (width, height))
    };
    let Ok(areas) = native.areas.try_borrow() else {
        return sys::HITTEST_NORMAL;
    };
    if flags & sys::WINDOW_FULLSCREEN != 0 {
        return sys::HITTEST_NORMAL;
    }
    let at = Point::new(point.x as f32, point.y as f32);
    let resizes = flags & sys::WINDOW_RESIZABLE != 0
        && flags & (sys::WINDOW_MAXIMIZED | sys::WINDOW_MINIMIZED) == 0;
    let border = match resizes && flags & sys::WINDOW_BORDERLESS != 0 {
        true => border_edge(
            PixelSize {
                width: size.0.max(0) as u32,
                height: size.1.max(0) as u32,
            },
            at,
        ),
        false => None,
    };
    match border.map_or_else(|| area_at(&areas, at), WindowArea::Resize) {
        WindowArea::Move => sys::HITTEST_DRAGGABLE,
        WindowArea::Resize(edge) if resizes => hit_code(edge),
        _ => sys::HITTEST_NORMAL,
    }
}

impl Sdl3 {
    /// Installs the hit-test callback on a window once; `what` names the
    /// capability a driver without hit testing lacks.
    fn hit_testing(&self, native: &Rc<NativeWindow>, what: &'static str) -> Result<()> {
        if native.hit_tested.get() {
            return Ok(());
        }
        let data = Rc::as_ptr(native).cast_mut().cast::<c_void>();
        // SAFETY: the callback reads `native` through shared references
        // only, and SDL drops it with the window, which `native` destroys
        // when its last reference goes.
        let done = unsafe { (self.lib.set_window_hit_test)(native.ptr(), Some(hit_test), data) };
        if !done {
            return Err(Error::Unsupported(what));
        }
        native.hit_tested.set(true);
        Ok(())
    }

    pub(crate) fn set_decorations(&mut self, id: WindowId, decorated: bool) -> Result<()> {
        let window = self.ordinary(id)?;
        if !decorated {
            // The resize border of a borderless window is hit-tested.
            self.hit_testing(&window.native, "undecorated windows")?;
        }
        // SAFETY: changing this live window's border.
        let done = unsafe { (self.lib.set_window_bordered)(window.ptr(), decorated) };
        self.check(done)
    }

    pub(crate) fn set_areas(&mut self, id: WindowId, areas: &[(Rect, WindowArea)]) -> Result<()> {
        check_areas(areas)?;
        let window = self.ordinary(id)?;
        if !areas.is_empty() {
            self.hit_testing(&window.native, "window move and resize areas")?;
        }
        let mut kept = window.native.areas.borrow_mut();
        kept.clear();
        kept.extend_from_slice(areas);
        Ok(())
    }

    /// Hands SDL a copy of `icon`.
    pub(crate) fn set_icon(&mut self, id: WindowId, icon: Icon<'_>) -> Result<()> {
        let window = self.ordinary(id)?.ptr();
        let extents = (
            c_int::try_from(icon.width),
            c_int::try_from(icon.height),
            icon.width
                .checked_mul(4)
                .and_then(|pitch| c_int::try_from(pitch).ok()),
        );
        let (Ok(width), Ok(height), Some(pitch)) = extents else {
            return Err(Error::InvalidSize);
        };
        if !icon.is_valid() {
            return Err(Error::InvalidSize);
        }
        let lib = self.lib;
        // SAFETY: the surface borrows the icon's pixels, which SDL only
        // reads and copies before the surface is destroyed below.
        let done = unsafe {
            let pixels = icon.rgba.as_ptr().cast_mut().cast::<c_void>();
            let surface =
                (lib.create_surface_from)(width, height, sys::PIXELFORMAT_RGBA32, pixels, pitch);
            if surface.is_null() {
                return Err(Error::backend(sys::error(lib)));
            }
            let done = (lib.set_window_icon)(window, surface);
            (lib.destroy_surface)(surface);
            done
        };
        // The surface was valid, so a refusal is the driver's: SDL's Wayland
        // driver without `xdg-toplevel-icon-v1`, or one with no icons at all.
        match done {
            true => Ok(()),
            false => Err(Error::Unsupported("window icons")),
        }
    }
}
