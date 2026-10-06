//! Owned native surface leases for graphics APIs that retain platform handles.
#![allow(unsafe_code)]

use crate::{Error, PixelSize, PresentError, Wayland, WindowId, state::Shell};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use smithay_client_toolkit::compositor::FrameCallbackData;
use std::{marker::PhantomData, ptr::NonNull, rc::Rc};
use wayland_client::{Connection, Proxy};

/// Owned UI-thread lease keeping a `wl_surface` and its connection alive.
///
/// Removing its window from [`Wayland`] stops input and dispatch for that window;
/// the native objects remain alive until every lease and GPU presenter is dropped.
/// This keeps handles valid throughout a graphics surface's destruction.
#[derive(Clone, Debug)]
pub struct WindowSurface {
    window: Shell,
    connection: Connection,
    _thread: PhantomData<Rc<()>>,
}

impl HasWindowHandle for WindowSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let ptr =
            NonNull::new(self.window.wl_surface().id().as_ptr()).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(ptr.cast()));
        // SAFETY: Window owns the protocol surface and Connection keeps its
        // libwayland display alive. Neither is destroyed while self is borrowed.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for WindowSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let ptr = NonNull::new(self.connection.backend().display_ptr())
            .ok_or(HandleError::Unavailable)?;
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr.cast()));
        // SAFETY: the owned Connection retains this display until self is dropped.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

impl Wayland {
    /// Returns a lease suitable for constructing an independent GPU presenter.
    /// No SHM image is allocated. Drop the presenter before the last surface lease.
    pub fn window_surface(&self, id: WindowId) -> Result<WindowSurface, Error> {
        let window = self
            .state
            .windows
            .iter()
            .find(|window| window.id == id)
            .ok_or(Error::InvalidWindow)?;
        Ok(WindowSurface {
            window: window.window.clone(),
            connection: self.connection.clone(),
            _thread: PhantomData,
        })
    }

    /// Presents through an external graphics API, using native frame pacing.
    ///
    /// Called only after [`crate::Event::Redraw`]. `present` receives physical
    /// extent and must commit the same surface (for example via Vulkan present),
    /// returning true only when it submitted a presentation. No SHM pixels are
    /// allocated, attached or read back. Returns false without calling `present`
    /// before configure or while a compositor frame callback is outstanding.
    /// Errors and false results preserve the redraw request for a later retry.
    pub fn present_external<E>(
        &mut self,
        id: WindowId,
        present: impl FnOnce(PixelSize) -> Result<bool, E>,
    ) -> Result<bool, PresentError<E>> {
        let window = self
            .state
            .windows
            .iter_mut()
            .find(|window| window.id == id)
            .ok_or(PresentError::Platform(Error::InvalidWindow))?;
        window.redraw_queued = false;
        if !window.info.configured || window.frame_pending {
            return Ok(false);
        }
        let size = window.info.buffer_size().map_err(PresentError::Platform)?;
        let surface = window.window.wl_surface();
        window.apply_scale();
        // The external API commits this request with its buffer attachment. A
        // failed acquisition leaves it pending for the next successful commit.
        if !window.frame_requested {
            surface.frame(&self.qh, FrameCallbackData(surface.clone()));
            window.frame_requested = true;
        }
        match present(size) {
            Ok(true) => {
                window.frame_requested = false;
                window.frame_pending = true;
                window.dirty = false;
                Ok(true)
            }
            Ok(false) => {
                window.dirty = true;
                Ok(false)
            }
            Err(error) => {
                window.dirty = true;
                Err(PresentError::Draw(error))
            }
        }
    }
}
