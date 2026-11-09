//! Window management: size and its limits, resizability, modes, position,
//! stacking and close requests of ordinary windows.

use crate::native::{App, Runtime, Window, WindowOptions};
use aegle_platform::{PixelSize, Platform, WindowId, WindowMode};
use aegle_ui::{Point, Result, UiError};

/// See [`Window::on_close`].
pub(crate) type CloseHandler = Box<dyn FnMut() -> Result<bool>>;

fn pixels(size: Option<(u32, u32)>) -> Option<PixelSize> {
    size.map(|(width, height)| PixelSize { width, height })
}

/// Applies the management settings of `options` that differ from the
/// defaults to a window just created, the mode last, so a window restored
/// maximized keeps the size and position it returns to.
pub(crate) fn apply(runtime: &mut Runtime, id: WindowId, options: &WindowOptions) -> Result<()> {
    let backend = &mut runtime.backend;
    if options.min_size.is_some() || options.max_size.is_some() {
        backend.set_size_limits(id, pixels(options.min_size), pixels(options.max_size))?;
    }
    if !options.resizable {
        backend.set_resizable(id, false)?;
    }
    if let Some(position) = options.position {
        backend.set_position(id, position)?;
    }
    if options.always_on_top {
        backend.set_always_on_top(id, true)?;
    }
    if options.mode != WindowMode::Normal {
        backend.set_mode(id, options.mode)?;
    }
    Ok(())
}

impl App {
    /// Asks the window's close handler, outside every runtime borrow, and
    /// closes the window unless it declines. Popups the window system
    /// dismissed close without asking.
    pub(crate) fn close_requested(&self, id: WindowId) -> Result<()> {
        let handler = {
            let mut runtime = self.runtime.borrow_mut();
            let Some(entry) = runtime.windows.iter_mut().find(|entry| entry.id == id) else {
                return Ok(());
            };
            match entry.parent {
                Some(_) => None,
                None => entry.on_close.take(),
            }
        };
        let close = match handler {
            None => true,
            Some(mut handler) => {
                let outcome = handler();
                let mut runtime = self.runtime.borrow_mut();
                let entry = runtime.windows.iter_mut().find(|entry| entry.id == id);
                if let Some(entry) = entry {
                    entry.on_close.get_or_insert(handler);
                }
                drop(runtime);
                match outcome {
                    Ok(close) => close,
                    // A handler that failed, say while saving, keeps the window.
                    Err(error) => {
                        self.report(Err(error))?;
                        false
                    }
                }
            }
        };
        let mut runtime = self.runtime.borrow_mut();
        if close && runtime.windows.iter().any(|entry| entry.id == id) {
            runtime.close(id)?;
        }
        Ok(())
    }
}

impl Window {
    fn with<T>(&self, f: impl FnOnce(&mut Runtime) -> Result<T>) -> Result<T> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let mut runtime = runtime.borrow_mut();
        f(&mut runtime)
    }

    /// The logical content size of the latest configuration, as
    /// `(width, height)`.
    pub fn size(&self) -> Result<(u32, u32)> {
        self.with(|runtime| {
            let size = runtime.backend.window_info(self.id)?.size;
            Ok((size.width, size.height))
        })
    }

    /// Resizes the content to `width` × `height` logical pixels, within the
    /// size limits; the window system may adjust it. Fails for a window that
    /// is maximized, minimized or full screen: restore it first.
    pub fn set_size(&self, width: u32, height: u32) -> Result<()> {
        let size = PixelSize { width, height };
        self.with(|runtime| Ok(runtime.backend.set_size(self.id, size)?))
    }

    /// Bounds the content size the user can resize to, `None` leaving that
    /// side open, and moves the current size into the bounds. A minimum
    /// larger than the maximum fails.
    pub fn set_size_limits(&self, min: Option<(u32, u32)>, max: Option<(u32, u32)>) -> Result<()> {
        self.with(|runtime| {
            Ok(runtime
                .backend
                .set_size_limits(self.id, pixels(min), pixels(max))?)
        })
    }

    /// Whether the user can resize the window and maximize it from its title
    /// bar; [`Self::set_size`] and [`Self::set_mode`] still apply.
    pub fn set_resizable(&self, resizable: bool) -> Result<()> {
        self.with(|runtime| Ok(runtime.backend.set_resizable(self.id, resizable)?))
    }

    /// How the window occupies the screen now. Wayland never reports
    /// [`WindowMode::Minimized`].
    pub fn mode(&self) -> Result<WindowMode> {
        self.with(|runtime| Ok(runtime.backend.window_info(self.id)?.mode))
    }

    /// Maximizes, minimizes, restores or makes the window full screen. The
    /// transition may be animated and finish in a later dispatch; [`Self::mode`]
    /// reports it once reached.
    pub fn set_mode(&self, mode: WindowMode) -> Result<()> {
        self.with(|runtime| Ok(runtime.backend.set_mode(self.id, mode)?))
    }

    /// The top-left corner of the content in the desktop's logical
    /// coordinates, those of [`crate::OutputInfo::position`]. Wayland gives
    /// windows no position and fails.
    pub fn position(&self) -> Result<Point> {
        self.with(|runtime| Ok(runtime.backend.position(self.id)?))
    }

    /// Moves the content's top-left corner to `position`, as
    /// [`Self::position`] reports it. Fails on Wayland, and for a window
    /// that is maximized, minimized or full screen.
    pub fn set_position(&self, position: Point) -> Result<()> {
        self.with(|runtime| Ok(runtime.backend.set_position(self.id, position)?))
    }

    /// Keeps the window above ordinary windows. Fails on Wayland.
    pub fn set_always_on_top(&self, on_top: bool) -> Result<()> {
        self.with(|runtime| Ok(runtime.backend.set_always_on_top(self.id, on_top)?))
    }

    /// Runs `handler` when the user asks to close the window, from its close
    /// button or the system's shortcut, outside every runtime borrow;
    /// returning `Ok(false)` keeps the window open, as when asking to save.
    /// An error goes to [`App::on_error`] and keeps the window open.
    /// [`Self::close`] closes without asking, and popups the window system
    /// dismisses never ask. Replaces the previous handler.
    pub fn on_close(&self, handler: impl FnMut() -> Result<bool> + 'static) -> Result<()> {
        self.with(|runtime| {
            let entry = runtime.windows.iter_mut().find(|entry| entry.id == self.id);
            entry.ok_or(UiError::DeadHandle)?.on_close = Some(Box::new(handler));
            Ok(())
        })
    }
}
