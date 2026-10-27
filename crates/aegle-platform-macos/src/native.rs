//! One window's retained state and the translations every callback shares.
use crate::{SEAT, frame::Frame, ime::Composition, shared::Shared, view::View};
use aegle_platform::{Cursor, Event, Modifiers, PixelSize, Point, WindowId, WindowInfo};
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSEvent, NSWindow};
use objc2_foundation::{MainThreadMarker, NSPoint};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

/// A window and everything keyed to it. The platform and each surface lease
/// hold it through `Rc`; the view reaches it through a `Weak`, so the window
/// and view are released with the last lease.
pub(crate) struct Native {
    pub id: WindowId,
    pub shared: Rc<Shared>,
    pub window: Retained<NSWindow>,
    pub view: Retained<View>,
    pub info: Cell<WindowInfo>,
    /// Whether callbacks still report; cleared when the window is removed.
    pub registered: Cell<bool>,
    /// Whether the first frame was presented and the window ordered front.
    pub shown: Cell<bool>,
    /// Whether the window is covered or on another space once shown.
    pub occluded: Cell<bool>,
    pub dirty: Cell<bool>,
    pub redraw_queued: Cell<bool>,
    /// When the last frame was presented, for frame pacing.
    pub last_frame: Cell<Option<Instant>>,
    /// Shape shown over the content, applied by the view's cursor rectangle.
    pub cursor: Cell<Cursor>,
    /// Whether the pointer is over the content.
    pub inside: Cell<bool>,
    /// Held mouse buttons: bit 0 primary, then secondary, middle, back, forward.
    pub held: Cell<u8>,
    /// Modifiers last reported to the host, so pointer events report changes.
    pub modifiers: Cell<Modifiers>,
    /// The focused editor's input-method session.
    pub ime: RefCell<Composition>,
    /// Caret area of that editor in window logical coordinates, top left origin.
    pub caret: Cell<aegle_platform::Rect>,
    pub frame: RefCell<Frame>,
    pub budget: usize,
    /// The host's answer for the current drag, `None` before one.
    pub drag_accept: Cell<Option<bool>>,
    /// Whether the current drag carries text or files.
    pub drag_active: Cell<bool>,
    /// Last reported drag position, to report motion only.
    pub drag_at: Cell<Option<Point>>,
    /// The latest mouse press or drag, which authorizes a drag session.
    pub last_mouse: RefCell<Option<Retained<NSEvent>>>,
}

/// Milliseconds of an event's timestamp, wrapping like the contract's times.
pub(crate) fn event_time(event: &NSEvent) -> u32 {
    (event.timestamp() * 1000.0) as u64 as u32
}

impl Native {
    /// Enables, updates or disables the composition for the focused editor.
    pub(crate) fn configure_ime(
        &self,
        request: Option<aegle_platform::ImeRequest>,
    ) -> aegle_platform::Result<()> {
        let Some(request) = request else {
            self.cancel_ime();
            return Ok(());
        };
        request.validate()?;
        self.caret.set(request.cursor_rect);
        let first = self.ime.borrow_mut().focus(
            request.surrounding.as_deref().unwrap_or_default(),
            request.anchor,
            request.cursor,
        );
        if let Some(context) = self.view.inputContext() {
            context.invalidateCharacterCoordinates();
        }
        if first && self.info.get().active {
            self.emit_ime(aegle_platform::ImeEvent::Entered);
        }
        Ok(())
    }

    /// Ends the editor session; queued updates belong to the old editor.
    pub(crate) fn cancel_ime(&self) {
        let composing = self.ime.borrow_mut().blur();
        if composing && let Some(context) = self.view.inputContext() {
            context.discardMarkedText();
        }
        let id = self.id;
        self.shared.events.borrow_mut().retain(|event| {
            !matches!(event, Event::Ime { window, event: aegle_platform::ImeEvent::Update(_), .. } if *window == id)
        });
    }

    pub(crate) fn emit_ime(&self, event: aegle_platform::ImeEvent) {
        self.emit(Event::Ime {
            window: self.id,
            seat: SEAT,
            event,
        });
    }

    /// Whether an input method is composing in this window.
    pub(crate) fn composing(&self) -> bool {
        self.ime.borrow().is_composing()
    }

    pub fn emit(&self, event: Event) {
        if self.registered.get() {
            self.shared.emit(event);
        }
    }

    /// The window's geometry as AppKit reports it now.
    pub fn measure(&self) -> WindowInfo {
        let scale = self.window.backingScaleFactor();
        let bounds = self.view.bounds().size;
        let physical = |points: f64| (points * scale).round().max(0.0) as u32;
        let buffer = PixelSize {
            width: physical(bounds.width),
            height: physical(bounds.height),
        };
        let scale = scale as f32;
        WindowInfo {
            size: PixelSize {
                width: (buffer.width as f32 / scale).ceil() as u32,
                height: (buffer.height as f32 / scale).ceil() as u32,
            },
            buffer,
            scale,
            active: self.window.isKeyWindow(),
            configured: !self.window.isMiniaturized() && buffer.width > 0 && buffer.height > 0,
        }
    }

    /// Reports a change of size, scale, activation or minimization, and
    /// repaints for it.
    pub fn refresh(&self) {
        let info = self.measure();
        if self.info.replace(info) != info {
            self.dirty.set(true);
            self.emit(Event::Configure {
                window: self.id,
                info,
            });
        }
    }

    /// The window-local logical position of `point`, given in window
    /// coordinates, with the origin moved to the top left.
    pub fn position(&self, point: NSPoint) -> Point {
        let local = self.view.convertPoint_fromView(point, None);
        let height = self.view.bounds().size.height;
        Point::new(local.x as f32, (height - local.y) as f32)
    }

    /// Emits `Event::Modifiers` when `modifiers` differ from the last report.
    pub fn sync_modifiers(&self, modifiers: Modifiers) {
        if self.modifiers.replace(modifiers) != modifiers {
            self.emit(Event::Modifiers {
                window: self.id,
                seat: SEAT,
                modifiers,
            });
        }
    }

    /// The time between frames of the screen showing the window.
    fn frame_interval(&self) -> Duration {
        let rate = self
            .window
            .screen()
            .map_or(60, |screen| screen.maximumFramesPerSecond());
        Duration::from_secs_f64(1.0 / rate.clamp(30, 240) as f64)
    }

    /// Queues the window's redraw when it is due. Returns when to try again
    /// if the frame interval has not yet passed, so animation runs at the
    /// screen's rate without an idle timer.
    pub fn queue_redraw(&self, now: Instant) -> Option<Instant> {
        let info = self.info.get();
        let hidden = self.shown.get() && self.occluded.get();
        if !self.registered.get()
            || !info.configured
            || !self.dirty.get()
            || self.redraw_queued.get()
            || hidden
        {
            return None;
        }
        let due = self.last_frame.get().map(|at| at + self.frame_interval());
        match due {
            Some(due) if due > now => Some(due),
            _ => {
                self.redraw_queued.set(true);
                self.emit(Event::Redraw { window: self.id });
                None
            }
        }
    }

    /// Orders the window front and active once its first frame exists, so
    /// accessibility adapters and GPU surfaces attach while it is hidden.
    pub fn show(&self, mtm: MainThreadMarker) {
        if self.shown.replace(true) {
            return;
        }
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        self.window.makeKeyAndOrderFront(None);
        self.refresh();
    }
}
