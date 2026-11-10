//! Window management of xdg toplevels: content size and its limits,
//! resizability and modes.
//!
//! A floating toplevel chooses its own size, so resizing presents at the new
//! extent. Limits are `xdg_toplevel` size hints, and a window the user cannot
//! resize pins both to its size. Modes are requests the compositor answers
//! with a configure, which reports maximized and full screen; it never
//! reports a minimized window. Wayland gives clients no positions and no
//! stacking, so those requests fail with [`Error::Unsupported`] through the
//! contract's defaults.

use crate::{Wayland, state::Shell};
use aegle_platform::{Error, Event, PixelSize, Result, WindowId, WindowMode, check_size_limits};
use smithay_client_toolkit::shell::{WaylandSurface, xdg::window::Window};

/// `xdg_toplevel`'s size hint for `size`, which takes positive `i32` extents.
fn hint(size: Option<PixelSize>) -> Option<(u32, u32)> {
    size.map(|size| {
        (
            size.width.min(i32::MAX as u32),
            size.height.min(i32::MAX as u32),
        )
    })
}

impl Wayland {
    /// A toplevel's window and its state; layers and popups are placed by
    /// their own protocols.
    pub(crate) fn toplevel(
        &mut self,
        id: WindowId,
    ) -> Result<(Window, &mut crate::state::WindowState)> {
        let window = self.window_mut(id)?;
        match &window.window {
            Shell::Xdg(native) => Ok((native.clone(), window)),
            _ => Err(Error::InvalidRole(
                "layers and popups are placed by their protocol",
            )),
        }
    }

    /// Sends a toplevel's effective size hints: its limits, or its size on
    /// both sides while the user may not resize it.
    fn apply_limits(&mut self, id: WindowId) -> Result<()> {
        let (native, window) = self.toplevel(id)?;
        let (min, max) = match window.resizable {
            true => window.limits,
            false => (Some(window.floating), Some(window.floating)),
        };
        native.set_min_size(hint(min));
        native.set_max_size(hint(max));
        // Size hints are double-buffered surface state.
        native.wl_surface().commit();
        Ok(())
    }

    pub(crate) fn set_size(&mut self, id: WindowId, size: PixelSize) -> Result<()> {
        let (_, window) = self.toplevel(id)?;
        if window.info.mode != WindowMode::Normal {
            return Err(Error::InvalidRole(
                "restore the window before moving or resizing it",
            ));
        }
        let (min, max) = window.limits;
        let mut info = window.info;
        info.size = size.clamp(min, max);
        crate::events::rescale(&mut info);
        if size.is_empty() || info.buffer.is_empty() {
            return Err(Error::InvalidSize);
        }
        window.info = info;
        window.floating = info.size;
        window.dirty = true;
        self.state
            .events
            .push_back(Event::Configure { window: id, info });
        if !self.window_mut(id)?.resizable {
            self.apply_limits(id)?;
        }
        Ok(())
    }

    pub(crate) fn set_size_limits(
        &mut self,
        id: WindowId,
        min: Option<PixelSize>,
        max: Option<PixelSize>,
    ) -> Result<()> {
        check_size_limits(min, max)?;
        let (_, window) = self.toplevel(id)?;
        window.limits = (min, max);
        let (size, mode) = (window.info.size, window.info.mode);
        self.apply_limits(id)?;
        if mode == WindowMode::Normal && size.clamp(min, max) != size {
            self.set_size(id, size)?;
        }
        Ok(())
    }

    pub(crate) fn set_resizable(&mut self, id: WindowId, resizable: bool) -> Result<()> {
        self.toplevel(id)?.1.resizable = resizable;
        self.apply_limits(id)
    }

    pub(crate) fn set_mode(&mut self, id: WindowId, mode: WindowMode) -> Result<()> {
        let (native, _) = self.toplevel(id)?;
        match mode {
            WindowMode::Fullscreen => native.set_fullscreen(None),
            WindowMode::Maximized => {
                native.unset_fullscreen();
                native.set_maximized();
            }
            WindowMode::Minimized => native.set_minimized(),
            _ => {
                native.unset_fullscreen();
                native.unset_maximized();
            }
        }
        Ok(())
    }
}
