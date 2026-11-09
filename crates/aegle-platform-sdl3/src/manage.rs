//! Window management: content size and its limits, resizability, modes,
//! position and stacking of ordinary windows through SDL's own calls.
//!
//! SDL applies most requests asynchronously; the window events they cause
//! report the outcome. Its Wayland driver cannot position windows or keep
//! them on top, as the protocol gives clients no such requests, so those
//! fail with [`Error::Unsupported`] there instead of being dropped by SDL.
#![allow(unsafe_code)]

use crate::{
    sys,
    window::{Sdl3, WindowState},
};
use aegle_platform::{Error, PixelSize, Point, Result, WindowId, WindowMode, check_size_limits};
use std::ffi::c_int;

/// The mode SDL's window flags describe.
pub(crate) fn mode(flags: u64) -> WindowMode {
    if flags & sys::WINDOW_FULLSCREEN != 0 {
        WindowMode::Fullscreen
    } else if flags & sys::WINDOW_MINIMIZED != 0 {
        WindowMode::Minimized
    } else if flags & sys::WINDOW_MAXIMIZED != 0 {
        WindowMode::Maximized
    } else {
        WindowMode::Normal
    }
}

/// `size` as SDL's signed extent, `(0, 0)` for none.
fn extent(size: Option<PixelSize>) -> Result<(c_int, c_int)> {
    let Some(size) = size else {
        return Ok((0, 0));
    };
    let convert = |value: u32| c_int::try_from(value).map_err(|_| Error::InvalidSize);
    if size.is_empty() {
        return Err(Error::InvalidSize);
    }
    Ok((convert(size.width)?, convert(size.height)?))
}

impl Sdl3 {
    /// An ordinary window, not a popup, which its anchor places.
    fn ordinary(&self, id: WindowId) -> Result<&WindowState> {
        let window = self.window(id)?;
        if window.parent.is_some() {
            return Err(Error::InvalidRole("a popup is placed by its anchor"));
        }
        Ok(window)
    }

    /// An ordinary window SDL shows in [`WindowMode::Normal`].
    fn normal(&self, id: WindowId) -> Result<&WindowState> {
        let window = self.ordinary(id)?;
        // SAFETY: a flag query of this live window.
        if mode(unsafe { (self.lib.get_window_flags)(window.ptr()) }) != WindowMode::Normal {
            return Err(Error::InvalidRole(
                "restore the window before moving or resizing it",
            ));
        }
        Ok(window)
    }

    /// Fails with SDL's message when a call returned false.
    fn check(&self, done: bool) -> Result<()> {
        match done {
            true => Ok(()),
            false => Err(Error::backend(sys::error(self.lib))),
        }
    }

    /// Positions and stacking need a driver whose protocol has them.
    fn placing(&self, what: &'static str) -> Result<()> {
        match self.wayland {
            true => Err(Error::Unsupported(what)),
            false => Ok(()),
        }
    }

    pub(crate) fn set_size(&mut self, id: WindowId, size: PixelSize) -> Result<()> {
        let window = self.normal(id)?;
        let (min, max) = window.limits;
        let (width, height) = extent(Some(size.clamp(min, max)))?;
        // SAFETY: resizing this live window.
        let done = unsafe { (self.lib.set_window_size)(window.ptr(), width, height) };
        self.check(done)
    }

    pub(crate) fn set_size_limits(
        &mut self,
        id: WindowId,
        min: Option<PixelSize>,
        max: Option<PixelSize>,
    ) -> Result<()> {
        check_size_limits(min, max)?;
        let (min_width, min_height) = extent(min)?;
        let (max_width, max_height) = extent(max.map(|max| PixelSize {
            width: max.width.min(c_int::MAX as u32),
            height: max.height.min(c_int::MAX as u32),
        }))?;
        let window = self.ordinary(id)?.ptr();
        // SAFETY: changing this live window's limits; zero means none.
        let done = unsafe {
            (self.lib.set_window_minimum_size)(window, min_width, min_height)
                && (self.lib.set_window_maximum_size)(window, max_width, max_height)
        };
        self.check(done)?;
        let state = self.window_mut(id)?;
        state.limits = (min, max);
        let size = state.info.size;
        if self.normal(id).is_ok() && size.clamp(min, max) != size {
            self.set_size(id, size)?;
        }
        Ok(())
    }

    pub(crate) fn set_resizable(&mut self, id: WindowId, resizable: bool) -> Result<()> {
        let window = self.ordinary(id)?.ptr();
        // SAFETY: changing this live window's border.
        let done = unsafe { (self.lib.set_window_resizable)(window, resizable) };
        self.check(done)
    }

    pub(crate) fn set_mode(&mut self, id: WindowId, target: WindowMode) -> Result<()> {
        let window = self.ordinary(id)?.ptr();
        let lib = self.lib;
        // SAFETY: state changes of this live window.
        let done = unsafe {
            let current = mode((lib.get_window_flags)(window));
            if current == target {
                return Ok(());
            }
            let left = match current {
                WindowMode::Fullscreen => (lib.set_window_fullscreen)(window, false),
                _ => true,
            };
            left && match target {
                WindowMode::Fullscreen => (lib.set_window_fullscreen)(window, true),
                WindowMode::Maximized => (lib.maximize_window)(window),
                WindowMode::Minimized => (lib.minimize_window)(window),
                _ => (lib.restore_window)(window),
            }
        };
        self.check(done)
    }

    pub(crate) fn position(&self, id: WindowId) -> Result<Point> {
        self.placing("window positions")?;
        let window = self.ordinary(id)?.ptr();
        let (mut x, mut y) = (0, 0);
        // SAFETY: a position query of this live window into local values.
        let done = unsafe { (self.lib.get_window_position)(window, &mut x, &mut y) };
        self.check(done)?;
        Ok(Point::new(x as f32, y as f32))
    }

    pub(crate) fn set_position(&mut self, id: WindowId, position: Point) -> Result<()> {
        self.placing("window positions")?;
        let window = self.normal(id)?.ptr();
        let convert = |value: f32| {
            let value = value.round();
            (value.is_finite() && value.abs() <= c_int::MAX as f32 / 2.0).then_some(value as c_int)
        };
        let (Some(x), Some(y)) = (convert(position.x), convert(position.y)) else {
            return Err(Error::InvalidPosition);
        };
        // SAFETY: moving this live window.
        let done = unsafe { (self.lib.set_window_position)(window, x, y) };
        self.check(done)
    }

    pub(crate) fn set_always_on_top(&mut self, id: WindowId, on_top: bool) -> Result<()> {
        self.placing("always-on-top windows")?;
        let window = self.ordinary(id)?.ptr();
        // SAFETY: changing this live window's stacking.
        let done = unsafe { (self.lib.set_window_always_on_top)(window, on_top) };
        self.check(done)
    }
}
