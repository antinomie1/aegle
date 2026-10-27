//! macOS: no desktop services are implemented yet.
//!
//! AppKit shows panels, notifications, status items and menu shortcuts only on
//! the main thread, which the application's own event loop owns. Rather than
//! block that loop or start a second AppKit session on a background thread,
//! every request reports [`Event::Unavailable`] with the reason; the caller
//! keeps working without the service, as it does for a Linux desktop without
//! a portal.
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use crate::{Event, FileDialog, Notification, Shortcut, Tray};

pub(crate) type Sink = Box<dyn Fn(Event) + Send + Sync>;

/// The application's desktop services; ids count requests so callers can
/// match the refusals they receive.
pub(crate) struct Desktop {
    sink: Arc<Sink>,
    next: AtomicU32,
}

impl Desktop {
    pub fn new(_app_id: &str, sink: Sink) -> io::Result<Self> {
        Ok(Self {
            sink: Arc::new(sink),
            next: AtomicU32::new(0),
        })
    }

    fn id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn refuse(&self, what: &str) {
        (self.sink)(Event::Unavailable(format!(
            "{what} is not implemented for macOS yet"
        )));
    }

    pub fn file_dialog(&self, _dialog: &FileDialog<'_>, _save: bool) -> io::Result<u32> {
        self.refuse("the native file dialog");
        Ok(self.id())
    }

    pub fn notify(&self, _notification: &Notification<'_>) -> io::Result<u32> {
        self.refuse("desktop notifications");
        Ok(self.id())
    }

    pub fn set_tray(&self, _tray: Option<&Tray<'_>>) -> io::Result<()> {
        self.refuse("the status item");
        Ok(())
    }

    pub fn bind_shortcuts(&self, _shortcuts: &[Shortcut<'_>]) -> io::Result<()> {
        self.refuse("global shortcuts");
        Ok(())
    }
}
