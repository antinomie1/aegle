//! Window management: content size and its limits, resizability, modes,
//! position and stacking level of ordinary windows.
//!
//! Modes change through AppKit's own transitions: zooming for
//! [`WindowMode::Maximized`], the Dock for [`WindowMode::Minimized`] and a
//! full-screen space for [`WindowMode::Fullscreen`]. Leaving full screen and
//! deminiaturizing are animated and finish in a delegate callback, so a
//! requested mode is kept as a target that each of those callbacks advances
//! one step, and a window not yet shown takes it when first shown.
#![allow(unsafe_code)] // Scheduling a block on the main run loop.

use crate::{MacOs, native::Native, outputs, window::MAX_EDGE};
use aegle_platform::{Error, PixelSize, Point, Result, WindowId, WindowMode, check_size_limits};
use objc2::DefinedClass;
use objc2_app_kit::{NSFloatingWindowLevel, NSNormalWindowLevel, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRunLoop, NSSize};
use std::rc::Rc;

/// The size a window's content can take: nonzero and within the backing store.
fn check_extent(size: PixelSize) -> Result<()> {
    if size.is_empty() || size.width > MAX_EDGE || size.height > MAX_EDGE {
        Err(Error::InvalidSize)
    } else {
        Ok(())
    }
}

fn ns_size(size: PixelSize) -> NSSize {
    NSSize::new(f64::from(size.width), f64::from(size.height))
}

impl MacOs {
    /// The window a management request applies to: not a popup, which its
    /// anchor places.
    pub(crate) fn ordinary(&self, id: WindowId) -> Result<&Rc<Native>> {
        let native = self.window(id)?;
        if native.parent.is_some() {
            return Err(Error::InvalidRole("a popup is placed by its anchor"));
        }
        Ok(native)
    }

    /// An ordinary window in [`WindowMode::Normal`], which may move and resize.
    fn normal(&self, id: WindowId) -> Result<&Rc<Native>> {
        let native = self.ordinary(id)?;
        if native.measure().mode != WindowMode::Normal {
            return Err(Error::InvalidRole(
                "restore the window before moving or resizing it",
            ));
        }
        Ok(native)
    }

    pub(crate) fn set_size(&mut self, id: WindowId, size: PixelSize) -> Result<()> {
        check_extent(size)?;
        let native = self.normal(id)?;
        let (min, max) = native.limits.get();
        native.window.setContentSize(ns_size(size.clamp(min, max)));
        native.refresh();
        Ok(())
    }

    pub(crate) fn set_size_limits(
        &mut self,
        id: WindowId,
        min: Option<PixelSize>,
        max: Option<PixelSize>,
    ) -> Result<()> {
        check_size_limits(min, max)?;
        if let Some(min) = min {
            check_extent(min)?;
        }
        let native = self.ordinary(id)?;
        native.limits.set((min, max));
        native
            .window
            .setContentMinSize(ns_size(min.unwrap_or_default()));
        native
            .window
            .setContentMaxSize(max.map_or(NSSize::new(f64::MAX, f64::MAX), ns_size));
        let info = native.measure();
        let clamped = info.size.clamp(min, max);
        if info.mode == WindowMode::Normal && clamped != info.size {
            native.window.setContentSize(ns_size(clamped));
            native.refresh();
        }
        Ok(())
    }

    pub(crate) fn set_resizable(&mut self, id: WindowId, resizable: bool) -> Result<()> {
        let native = self.ordinary(id)?;
        let mut style = native.window.styleMask();
        if style.contains(NSWindowStyleMask::Resizable) != resizable {
            style.set(NSWindowStyleMask::Resizable, resizable);
            native.restyle(style);
        }
        Ok(())
    }

    pub(crate) fn set_mode(&mut self, id: WindowId, mode: WindowMode) -> Result<()> {
        let native = self.ordinary(id)?;
        native.target_mode.set(Some(mode));
        native.advance_mode();
        Ok(())
    }

    pub(crate) fn position(&self, id: WindowId) -> Result<Point> {
        let window = &self.ordinary(id)?.window;
        let content = window.contentRectForFrameRect(window.frame());
        Ok(outputs::to_desktop(self.mtm, content).origin)
    }

    pub(crate) fn set_position(&mut self, id: WindowId, position: Point) -> Result<()> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(Error::InvalidPosition);
        }
        let window = &self.normal(id)?.window;
        let frame = window.frame();
        let content = window.contentRectForFrameRect(frame);
        let (x, bottom) = outputs::from_desktop(self.mtm, position, content.size.height);
        // The title bar sits above the content, so the frame's origin keeps
        // the content's offset inside it.
        window.setFrameOrigin(NSPoint::new(
            x - (content.origin.x - frame.origin.x),
            bottom - (content.origin.y - frame.origin.y),
        ));
        Ok(())
    }

    pub(crate) fn set_always_on_top(&mut self, id: WindowId, on_top: bool) -> Result<()> {
        let level = match on_top {
            true => NSFloatingWindowLevel,
            false => NSNormalWindowLevel,
        };
        self.ordinary(id)?.window.setLevel(level);
        Ok(())
    }
}

impl Native {
    /// The mode AppKit shows the window in now.
    pub(crate) fn mode(&self) -> WindowMode {
        if self.parent.is_some() {
            WindowMode::Normal
        } else if self
            .window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen)
        {
            WindowMode::Fullscreen
        } else if self.window.isMiniaturized() {
            WindowMode::Minimized
        } else if self.is_zoomed() {
            WindowMode::Maximized
        } else {
            WindowMode::Normal
        }
    }

    /// Takes one step toward the requested mode once the window is shown and
    /// no full-screen transition runs. Leaving full screen and the Dock end
    /// in callbacks that schedule the next step; the step that reaches the
    /// target clears it, so a request AppKit declines is not retried later.
    pub(crate) fn advance_mode(&self) {
        let Some(target) = self.target_mode.get() else {
            return;
        };
        if !self.shown.get() || self.transitioning.get() {
            return;
        }
        let current = self.mode();
        if current == target {
            self.target_mode.set(None);
            return;
        }
        match (current, target) {
            (WindowMode::Fullscreen, _) => self.toggle_full_screen(),
            (WindowMode::Minimized, _) => self.window.deminiaturize(None),
            (_, WindowMode::Fullscreen) => {
                self.target_mode.set(None);
                self.toggle_full_screen();
            }
            (_, WindowMode::Minimized) => {
                self.target_mode.set(None);
                self.window.miniaturize(None);
            }
            _ => {
                self.target_mode.set(None);
                self.zoom_to(target == WindowMode::Maximized);
                self.refresh();
            }
        }
    }

    /// Whether the window is zoomed: a zoom brought it to the screen's frame
    /// and it is still there. AppKit alone also calls a window zoomed that
    /// sits at its maximum size or that the user cannot resize.
    fn is_zoomed(&self) -> bool {
        self.zoomed.get() && (!self.resizable() || self.window.isZoomed())
    }

    /// AppKit is about to zoom the window at the user's request or ours.
    pub(crate) fn will_zoom(&self) {
        self.zoomed.set(!self.window.isZoomed());
    }

    /// Whether the user can resize the window.
    pub(crate) fn resizable(&self) -> bool {
        self.window
            .styleMask()
            .contains(NSWindowStyleMask::Resizable)
    }

    /// Changes the window's style. Restyling hands the first responder back
    /// to the window, which would end key delivery to the view.
    pub(crate) fn restyle(&self, style: NSWindowStyleMask) {
        self.window.setStyleMask(style);
        self.window.makeFirstResponder(Some(&self.view));
    }

    /// Zooms the window to the screen's frame or back to the user's. AppKit
    /// zooms only windows the user can resize, so a fixed one is resizable
    /// meanwhile.
    fn zoom_to(&self, zoomed: bool) {
        let style = self.window.styleMask();
        let fixed = !self.resizable();
        if fixed {
            self.window
                .setStyleMask(style | NSWindowStyleMask::Resizable);
        }
        if self.window.isZoomed() != zoomed {
            self.window.zoom(None);
        }
        if fixed {
            self.restyle(style);
        }
        self.zoomed.set(zoomed);
    }

    fn toggle_full_screen(&self) {
        // The will-callbacks mark the transition, so a toggle AppKit refuses
        // leaves nothing waiting.
        self.window.toggleFullScreen(None);
    }

    /// A full-screen transition ended, completed or not.
    pub(crate) fn full_screen_changed(&self) {
        self.transitioning.set(false);
        self.apply_decorations();
        self.dirty.set(true);
        self.refresh();
        self.advance_later();
    }

    /// Takes the next step on a later turn of the run loop: AppKit refuses a
    /// new full-screen transition from the callback that ends the last one.
    pub(crate) fn advance_later(&self) {
        if self.target_mode.get().is_none() {
            return;
        }
        let native = self.view.ivars().clone();
        let block = block2::RcBlock::new(move || {
            if let Some(native) = native.upgrade().filter(|n| n.registered.get()) {
                native.advance_mode();
            }
        });
        // SAFETY: the main run loop runs the block on the main thread, where
        // it was made and where everything it captures lives.
        unsafe { NSRunLoop::mainRunLoop().performBlock(&block) };
    }
}
