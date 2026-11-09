//! The platform contract on the AppKit event loop. macOS has one seat,
//! [`crate::SEAT`], screens as outputs, popups as child windows and no
//! layer surfaces.
#![allow(unsafe_code)] // AppKit constants and the deprecated activation call need `unsafe`.

use crate::{MacOs, wake::Waker, window::WindowSurface};
use aegle_platform::{
    Cursor, DragData, DrawResult, Error, Event, ImeRequest, OutputInfo, PixelRect, PixelSize,
    Platform, Point, Preferences, Region, Result, SeatId, SurfacePlatform, WakeHandle, WindowId,
    WindowInfo, WindowMode, WindowOptions,
};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSEventMask, NSEventType};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode};
use std::time::{Duration, Instant};

impl MacOs {
    /// Waits for native work until `timeout` elapses or, without one, until
    /// something arrives, then returns. Frames that come due while waiting
    /// are queued before returning, so animation keeps the screen's rate
    /// without an idle timer. Every call takes at least one turn of the run
    /// loop, even with events already queued: a host that redraws every turn
    /// must not starve AppKit, whose turns commit the window's layers and
    /// deliver its occlusion changes.
    pub(crate) fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()> {
        let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        // Each turn drains its own autorelease pool, as `-[NSApplication run]`
        // does: AppKit autoreleases events, dates and what drawing retains.
        while !autoreleasepool(|_| self.turn(deadline)) {}
        Ok(())
    }

    /// Waits for and delivers one native event. Returns whether `dispatch`
    /// is done: the wait ended or the event queued something.
    fn turn(&mut self, deadline: Option<Instant>) -> bool {
        // Queues due frames; the next one bounds the wait below.
        let next_frame = self.refresh();
        let wait = if self.shared.events.borrow().is_empty() {
            [
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now())),
                next_frame.map(|frame| frame.saturating_duration_since(Instant::now())),
            ]
            .into_iter()
            .flatten()
            .min()
        } else {
            Some(Duration::ZERO)
        };
        // A nil date would not wait at all.
        let date = match wait {
            Some(left) => NSDate::dateWithTimeIntervalSinceNow(left.as_secs_f64()),
            None => NSDate::distantFuture(),
        };
        self.shared.waiting.set(true);
        let event = self.app.nextEventMatchingMask_untilDate_inMode_dequeue(
            NSEventMask::Any,
            Some(&date),
            // SAFETY: a static AppKit run-loop mode constant.
            unsafe { NSDefaultRunLoopMode },
            true,
        );
        self.shared.waiting.set(false);
        let Some(event) = event else {
            // The wait ended with nothing to translate; report the frames
            // its deadline held.
            self.refresh();
            return true;
        };
        if event.r#type() == NSEventType::ApplicationDefined {
            // A callback during the fetch already queued its event.
            return false;
        }
        self.press_popups(&event);
        self.app.sendEvent(&event);
        self.refresh();
        !self.shared.events.borrow().is_empty()
    }

    /// Queues due frames. An occluded window waits for its occlusion callback
    /// instead. Returns when the next frame becomes due, if a window waits
    /// for one.
    pub(crate) fn refresh(&mut self) -> Option<Instant> {
        let now = Instant::now();
        let mut next: Option<Instant> = None;
        for native in &self.windows {
            next = match (native.queue_redraw(now), next) {
                (Some(due), Some(next)) => Some(due.min(next)),
                (Some(due), None) => Some(due),
                (None, next) => next,
            };
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

    fn outputs(&self) -> Result<Vec<OutputInfo>> {
        Ok(crate::outputs::outputs(self.mtm))
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

    fn set_size(&mut self, window: WindowId, size: PixelSize) -> Result<()> {
        MacOs::set_size(self, window, size)
    }

    fn set_size_limits(
        &mut self,
        window: WindowId,
        min: Option<PixelSize>,
        max: Option<PixelSize>,
    ) -> Result<()> {
        MacOs::set_size_limits(self, window, min, max)
    }

    fn set_resizable(&mut self, window: WindowId, resizable: bool) -> Result<()> {
        MacOs::set_resizable(self, window, resizable)
    }

    fn set_mode(&mut self, window: WindowId, mode: WindowMode) -> Result<()> {
        MacOs::set_mode(self, window, mode)
    }

    fn position(&self, window: WindowId) -> Result<Point> {
        MacOs::position(self, window)
    }

    fn set_position(&mut self, window: WindowId, position: Point) -> Result<()> {
        MacOs::set_position(self, window, position)
    }

    fn set_always_on_top(&mut self, window: WindowId, on_top: bool) -> Result<()> {
        MacOs::set_always_on_top(self, window, on_top)
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
        // Drawing autoreleases the frame's image; drain it with the frame.
        let presented = autoreleasepool(|_| native.present(mtm, damage, draw))?;
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
        // Metal autoreleases each drawable; drain it with the frame.
        let presented = autoreleasepool(|_| MacOs::present_external(self, window, present))?;
        if presented {
            self.refresh();
        }
        Ok(presented)
    }
}
