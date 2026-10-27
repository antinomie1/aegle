//! State every window and AppKit callback of one connection shares.
use aegle_platform::{Event, Preferences, WindowId};
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
}

impl Shared {
    pub fn new(preferences: Preferences) -> Self {
        Self {
            events: RefCell::new(VecDeque::new()),
            windows: RefCell::new(Vec::new()),
            preferences: Cell::new(preferences),
        }
    }

    /// Queues `event` unconditionally; windows add their registration check.
    pub fn emit(&self, event: Event) {
        self.events.borrow_mut().push_back(event);
    }

    /// Re-reads the system preferences and reports a change.
    pub fn refresh_preferences(&self, current: Preferences) {
        if self.preferences.replace(current) != current {
            self.emit(Event::Preferences(current));
        }
    }
}
