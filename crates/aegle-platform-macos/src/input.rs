//! Keyboard, mouse and scroll events as the contract's vocabulary.
#![allow(unsafe_code)] // Retaining an `NSEvent` for a later drag session needs `unsafe`.

use crate::{
    SEAT, keys,
    native::{Native, event_time},
};
use aegle_platform::{Event, Point, PointerButton, PointerKind};
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventPhase, NSEventType};
use objc2_foundation::NSArray;

/// Adds a reference to `event`, which a drag session started later needs.
fn retain(event: &NSEvent) -> Retained<NSEvent> {
    // SAFETY: a live event; the reference is released with the window.
    unsafe { Retained::retain(event as *const NSEvent as *mut NSEvent) }
        .unwrap_or_else(|| unreachable!("a live event cannot fail to be retained"))
}

/// Logical pixels one wheel notch or scroll line moves.
const LINE: f32 = 16.0;

/// The held-button bit and, for buttons other than the primary one, the
/// button of a mouse event; `None` for an unknown button.
fn button(event: &NSEvent) -> Option<(u8, Option<PointerButton>)> {
    let kind = event.r#type();
    if kind == NSEventType::LeftMouseDown || kind == NSEventType::LeftMouseUp {
        Some((1, None))
    } else if kind == NSEventType::RightMouseDown || kind == NSEventType::RightMouseUp {
        Some((2, Some(PointerButton::Secondary)))
    } else {
        match event.buttonNumber() {
            2 => Some((4, Some(PointerButton::Middle))),
            3 => Some((8, Some(PointerButton::Back))),
            4 => Some((16, Some(PointerButton::Forward))),
            _ => None,
        }
    }
}

impl Native {
    /// Queues one pointer event, first reporting modifier changes.
    fn pointer(&self, event: &NSEvent, kind: PointerKind) {
        self.sync_modifiers(keys::modifiers(event));
        self.emit(Event::Pointer {
            window: self.id,
            seat: SEAT,
            position: self.position(event.locationInWindow()),
            kind,
            time: Some(event_time(event)),
        });
    }

    pub(crate) fn mouse_down(&self, event: &NSEvent) {
        let Some((bit, button)) = button(event) else {
            return;
        };
        self.held.set(self.held.get() | bit);
        *self.last_mouse.borrow_mut() = Some(retain(event));
        let kind = match button {
            Some(button) => PointerKind::ButtonDown(button),
            None => PointerKind::Down {
                clicks: event.clickCount().clamp(1, isize::from(u8::MAX)) as u8,
            },
        };
        self.pointer(event, kind);
    }

    pub(crate) fn mouse_up(&self, event: &NSEvent) {
        let Some((bit, button)) = button(event) else {
            return;
        };
        let held = self.held.get();
        if held & bit == 0 {
            return;
        }
        self.held.set(held & !bit);
        self.pointer(event, button.map_or(PointerKind::Up, PointerKind::ButtonUp));
        // A press that began inside ends outside: the pointer has left.
        if held == bit && !self.contains(event) {
            self.inside.set(false);
            self.pointer(event, PointerKind::Leave);
        }
    }

    /// Whether the event's location is within the content.
    fn contains(&self, event: &NSEvent) -> bool {
        let at = self.position(event.locationInWindow());
        let bounds = self.view.bounds().size;
        at.x >= 0.0
            && at.y >= 0.0
            && f64::from(at.x) < bounds.width
            && f64::from(at.y) < bounds.height
    }

    /// Motion, with or without a button held; captured motion continues
    /// outside the window.
    pub(crate) fn mouse_move(&self, event: &NSEvent) {
        if self.held.get() != 0 {
            *self.last_mouse.borrow_mut() = Some(retain(event));
        }
        self.pointer(event, PointerKind::Move);
    }

    pub(crate) fn mouse_enter(&self, event: &NSEvent) {
        self.inside.set(true);
        self.pointer(event, PointerKind::Move);
    }

    pub(crate) fn mouse_exit(&self, event: &NSEvent) {
        self.inside.set(false);
        // A held button keeps the gesture; its release reports the leave.
        if self.held.get() == 0 {
            self.pointer(event, PointerKind::Leave);
        }
    }

    pub(crate) fn scroll(&self, event: &NSEvent) {
        let precise = event.hasPreciseScrollingDeltas();
        let phase = event.phase();
        // The host flings after the fingers lift, so the system's own
        // momentum events would scroll twice.
        if precise && !event.momentumPhase().is_empty() {
            return;
        }
        let unit = if precise { 1.0 } else { LINE };
        // AppKit reports content movement with the contract's sign: positive
        // means the content moves right or down, as the wheel and touchpad
        // settings define it.
        let delta = Point::new(
            (event.scrollingDeltaX() as f32) * unit,
            (event.scrollingDeltaY() as f32) * unit,
        );
        if delta != Point::new(0.0, 0.0) {
            self.pointer(event, PointerKind::Scroll { delta });
        }
        if precise && phase.intersects(NSEventPhase::Ended | NSEventPhase::Cancelled) {
            self.pointer(event, PointerKind::ScrollEnd);
        }
    }

    pub(crate) fn key_down(&self, event: &NSEvent) {
        let modifiers = keys::modifiers(event);
        let shortcut = modifiers.control || modifiers.meta;
        // An editor's input client translates the key: dead keys, input
        // methods and the character palette. Shortcuts bypass it, unless a
        // composition is running that they would otherwise disturb.
        let text = if self.ime_enabled() && (!shortcut || self.composing()) {
            self.begin_key();
            // The client's methods run now, without a borrow held here.
            self.view.interpretKeyEvents(&NSArray::from_slice(&[event]));
            let scratch = self.end_key();
            if scratch.consumed {
                return;
            }
            scratch.text
        } else if shortcut {
            None
        } else {
            keys::typed_text(event)
        };
        self.modifiers.set(modifiers);
        self.emit(Event::Key {
            window: self.id,
            seat: SEAT,
            key: keys::key(event),
            text,
            pressed: true,
            repeat: event.isARepeat(),
            modifiers,
            time: event_time(event),
        });
    }

    pub(crate) fn key_up(&self, event: &NSEvent) {
        let modifiers = keys::modifiers(event);
        self.modifiers.set(modifiers);
        self.emit(Event::Key {
            window: self.id,
            seat: SEAT,
            key: keys::key(event),
            text: None,
            pressed: false,
            repeat: false,
            modifiers,
            time: event_time(event),
        });
    }

    pub(crate) fn flags_changed(&self, event: &NSEvent) {
        self.sync_modifiers(keys::modifiers(event));
    }
}
