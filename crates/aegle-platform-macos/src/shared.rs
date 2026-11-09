//! State every window and AppKit callback of one connection shares.
use aegle_platform::{Event, Preferences, WindowId};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

/// The event queue and registry behind one [`crate::MacOs`], owned through
/// `Rc` by the platform, each window and the application delegate. Only the
/// main thread touches it; borrows are never held across an AppKit call, which
/// may call back.
pub(crate) struct Shared {
    pub events: RefCell<VecDeque<Event>>,
    /// Live windows in creation order.
    pub windows: RefCell<Vec<WindowId>>,
    /// Last reported preferences, so one system change emits once.
    pub preferences: Cell<Preferences>,
    /// Live popups in creation order, so a parent precedes its popups.
    pub popups: RefCell<Vec<crate::popup::Popup>>,
    /// The loop is blocked in its event fetch. AppKit services run-loop
    /// sources there (wakes, input methods committing asynchronously), and
    /// only an event ends the fetch, so the first callback to queue one posts it.
    pub waiting: Cell<bool>,
    mtm: MainThreadMarker,
}

impl Shared {
    pub fn new(preferences: Preferences, mtm: MainThreadMarker) -> Self {
        Self {
            events: RefCell::new(VecDeque::new()),
            windows: RefCell::new(Vec::new()),
            preferences: Cell::new(preferences),
            popups: RefCell::new(Vec::new()),
            waiting: Cell::new(false),
            mtm,
        }
    }

    /// Queues `event` unconditionally; windows add their registration check.
    pub fn emit(&self, event: Event) {
        self.events.borrow_mut().push_back(event);
        if self.waiting.replace(false) {
            let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::ApplicationDefined,
                NSPoint::ZERO,
                NSEventModifierFlags::empty(),
                0.0,
                0,
                None,
                0,
                0,
                0,
            );
            if let Some(event) = event {
                NSApplication::sharedApplication(self.mtm).postEvent_atStart(&event, true);
            }
        }
    }

    /// Re-reads the system preferences and reports a change.
    pub fn refresh_preferences(&self, current: Preferences) {
        if self.preferences.replace(current) != current {
            self.emit(Event::Preferences(current));
        }
    }
}
