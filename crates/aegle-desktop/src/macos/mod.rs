//! macOS: file panels, a status item and Carbon hot keys on the main thread.
//!
//! AppKit shows panels and status items only on the main thread, which the
//! application's event loop owns, so there is no service thread here: the
//! [`crate::Desktop`] is created and used on the main thread, and its events
//! come from AppKit callbacks while that loop runs, as `aegle-app`'s does.
//! Calls from another thread fail rather than block or start a second AppKit
//! session. Notifications go through `UNUserNotificationCenter`, which serves
//! application bundles only.
use std::{
    cell::RefCell,
    io,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use dispatch2::MainThreadBound;
use objc2_foundation::MainThreadMarker;

use crate::{Event, FileDialog, Notification, Shortcut, Tray};

mod files;
mod hotkeys;
mod notify;
mod tray;

pub(crate) type Sink = Box<dyn Fn(Event) + Send + Sync>;

/// The main-thread objects behind one [`Desktop`].
#[derive(Default)]
struct Main {
    tray: RefCell<Option<tray::StatusItem>>,
    hot_keys: RefCell<Option<hotkeys::HotKeys>>,
    notifier: RefCell<Option<notify::Notifier>>,
}

/// The application's desktop services.
pub(crate) struct Desktop {
    sink: Arc<Sink>,
    next: AtomicU32,
    /// Dropped on the main thread wherever the `Desktop` goes.
    main: MainThreadBound<Main>,
}

fn main_thread() -> io::Result<MainThreadMarker> {
    MainThreadMarker::new().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "macOS desktop services run on the main thread",
        )
    })
}

impl Desktop {
    pub fn new(_app_id: &str, sink: Sink) -> io::Result<Self> {
        let mtm = main_thread()?;
        Ok(Self {
            sink: Arc::new(sink),
            next: AtomicU32::new(0),
            main: MainThreadBound::new(Main::default(), mtm),
        })
    }

    fn id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn file_dialog(&self, dialog: &FileDialog<'_>, save: bool) -> io::Result<u32> {
        let mtm = main_thread()?;
        let request = self.id();
        files::show(mtm, &self.sink, request, dialog, save);
        Ok(request)
    }

    pub fn notify(&self, notification: &Notification<'_>) -> io::Result<u32> {
        let mtm = main_thread()?;
        let id = self.id();
        let mut notifier = self.main.get(mtm).notifier.borrow_mut();
        if notifier.is_none() {
            match notify::Notifier::new(self.sink.clone()) {
                Ok(created) => *notifier = Some(created),
                Err(reason) => {
                    (self.sink)(Event::Unavailable(reason));
                    return Ok(id);
                }
            }
        }
        if let Some(notifier) = notifier.as_mut() {
            notifier.notify(&self.sink, id, notification);
        }
        Ok(id)
    }

    pub fn set_tray(&self, tray: Option<&Tray<'_>>) -> io::Result<()> {
        let mtm = main_thread()?;
        let mut shown = self.main.get(mtm).tray.borrow_mut();
        match (tray, shown.as_ref()) {
            (Some(tray), Some(item)) => item.update(mtm, tray),
            (Some(tray), None) => {
                *shown = Some(tray::StatusItem::new(mtm, self.sink.clone(), tray))
            }
            (None, _) => *shown = None,
        }
        Ok(())
    }

    pub fn bind_shortcuts(&self, shortcuts: &[Shortcut<'_>]) -> io::Result<()> {
        let mtm = main_thread()?;
        let mut bound = self.main.get(mtm).hot_keys.borrow_mut();
        // The old keys go first, so a shortcut bound again can register.
        *bound = None;
        if !shortcuts.is_empty() {
            *bound = Some(hotkeys::HotKeys::new(self.sink.clone(), shortcuts));
        }
        Ok(())
    }
}
