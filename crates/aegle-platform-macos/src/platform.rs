//! The platform contract on the AppKit event loop. macOS has one seat,
//! [`crate::SEAT`], and no layer surfaces.
#![allow(unsafe_code)] // AppKit constants and the deprecated activation call need `unsafe`.

use crate::{MacOs, wake::Waker, window::WindowSurface};
use aegle_platform::{
    Cursor, DragData, DrawResult, Error, Event, ImeRequest, PixelRect, PixelSize, Platform,
    Preferences, Region, Result, SeatId, SurfacePlatform, WakeHandle, WindowId, WindowInfo,
    WindowOptions,
};
use objc2_app_kit::{NSEventMask, NSEventType};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode};
use std::time::{Duration, Instant};

impl MacOs {
    /// Waits for native work until `timeout` elapses or, without one, until
    /// something arrives, then returns. Frames that come due while waiting
    /// are queued before returning, so animation keeps the screen's rate
    /// without an idle timer.
    pub(crate) fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()> {
        let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        loop {
            // Queues due frames; the next one bounds the wait below.
            let next_frame = self.refresh();
            if !self.shared.events.borrow().is_empty() {
                return Ok(());
            }
            let wait = [
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now())),
                next_frame.map(|frame| frame.saturating_duration_since(Instant::now())),
            ]
            .into_iter()
            .flatten()
            .min();
            if wait.is_some_and(|left| left.is_zero()) {
                // The earliest deadline is now: report the frames it held.
                self.refresh();
                return Ok(());
            }
            let date = wait.map(|left| NSDate::dateWithTimeIntervalSinceNow(left.as_secs_f64()));
            let event = self.app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                date.as_deref(),
                // SAFETY: a static AppKit run-loop mode constant.
                unsafe { NSDefaultRunLoopMode },
                true,
            );
            let Some(event) = event else {
                // The wait ended with nothing to translate.
                self.refresh();
                return Ok(());
            };
            if event.r#type() == NSEventType::ApplicationDefined {
                // A wake; the signal's callback already queued its event.
                continue;
            }
            self.app.sendEvent(&event);
            self.refresh();
            if !self.shared.events.borrow().is_empty() {
                return Ok(());
            }
        }
    }

    /// Reports pending frames and system preferences. Returns when the next
    /// frame becomes due, if a window waits for one.
    pub(crate) fn refresh(&mut self) -> Option<Instant> {
        let now = Instant::now();
        let mut next: Option<Instant> = None;
        for native in &self.windows {
            next = match (native.queue_redraw(now), next) {
                (Some(due), Some(next)) => Some(due.min(next)),
                (Some(due), None) => Some(due),
                (None, next) => next,
            };
            // A window ordered in from another space reports itself visible
            // again through its occlusion callback; a hidden or minimized one
            // must not present.
            if native.shown.get()
                && native.occluded.get()
                && native.window.isVisible()
                && !native.window.isMiniaturized()
            {
                native.occluded.set(false);
                native.dirty.set(true);
            }
        }
        next
    }
}

impl Platform for MacOs {
    fn connect() -> Result<Self> {
        MacOs::connect()
    }

    fn preferences(&self) -> Preferences {
        self.shared.preferences.get()
    }

    fn wake_handle(&mut self) -> Result<WakeHandle> {
        if self.wake.is_none() {
            let shared = self.shared.clone();
            let (waker, wake) = Waker::new(&shared)
                .ok_or_else(|| Error::backend("the wake run-loop source could not be created"))?;
            self.waker = Some(waker);
            self.wake = Some(wake);
        }
        Ok(self.wake.clone().unwrap())
    }

    fn ime_available(&self) -> bool {
        true
    }

    fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId> {
        MacOs::create_window(self, options)
    }

    fn remove_window(&mut self, window: WindowId) -> Result<()> {
        MacOs::remove_window(self, window)
    }

    fn window_info(&self, window: WindowId) -> Result<WindowInfo> {
        Ok(self.window(window)?.info.get())
    }

    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()> {
        MacOs::dispatch(self, timeout)
    }

    fn next_event(&mut self) -> Option<Event> {
        self.shared.events.borrow_mut().pop_front()
    }

    fn request_redraw(&mut self, window: WindowId) -> Result<()> {
        self.window(window)?.dirty.set(true);
        Ok(())
    }

    fn set_title(&mut self, window: WindowId, title: &str) -> Result<()> {
        MacOs::set_title(self, window, title)
    }

    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()> {
        MacOs::set_cursor(self, window, cursor)
    }

    fn configure_ime(&mut self, window: WindowId, request: Option<ImeRequest>) -> Result<()> {
        self.window(window)?.configure_ime(request)
    }

    fn set_clipboard(&mut self, window: WindowId, _: SeatId, text: &str) -> Result<()> {
        self.window(window)?.set_clipboard(text)
    }

    fn request_clipboard(&mut self, window: WindowId, _: SeatId) -> Result<()> {
        self.window(window)?.request_clipboard();
        Ok(())
    }

    fn accept_drag(&mut self, window: WindowId, _: SeatId, accept: bool) -> Result<()> {
        self.window(window)?.drag_accept.set(Some(accept));
        Ok(())
    }

    fn start_drag(&mut self, window: WindowId, _: SeatId, data: &DragData) -> Result<()> {
        let mtm = self.mtm;
        self.window(window)?.start_drag(mtm, data)
    }

    fn present(
        &mut self,
        window: WindowId,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool> {
        let mtm = self.mtm;
        let native = self.window(window)?.clone();
        let presented = native.present(mtm, damage, draw)?;
        if presented {
            self.refresh();
        }
        Ok(presented)
    }
}

impl SurfacePlatform for MacOs {
    type Surface = WindowSurface;

    fn window_surface(&self, window: WindowId) -> Result<WindowSurface> {
        MacOs::window_surface(self, window)
    }

    fn present_external(
        &mut self,
        window: WindowId,
        present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool> {
        let presented = MacOs::present_external(self, window, present)?;
        if presented {
            self.refresh();
        }
        Ok(presented)
    }
}
