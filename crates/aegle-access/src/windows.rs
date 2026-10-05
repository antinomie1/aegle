#![allow(unsafe_code)]

use crate::{Event, Mailbox};
use accesskit::TreeUpdate;
use accesskit_windows::{HWND, SubclassingAdapter};
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle};

/// Optional UI Automation adapter retaining its native window lease.
///
/// Native callbacks enqueue requests without borrowing the application's UI.
/// AccessKit supplies a placeholder until the UI thread publishes the initial
/// tree. Window focus messages are handled by the upstream subclass itself.
pub struct WindowsAdapter<W> {
    // Drop the subclass before releasing the HWND lease.
    native: SubclassingAdapter,
    mailbox: Mailbox,
    _window: W,
}

impl<W: HasWindowHandle> WindowsAdapter<W> {
    /// Installs UI Automation before the window's first presentation.
    /// `wake` must signal the owning event loop without blocking.
    ///
    /// # Safety
    /// Call on the thread owning the still-hidden Win32 window. The supplied
    /// owner must keep that same HWND alive until this adapter is dropped on
    /// that thread. Do not replace its window procedure while installed.
    ///
    /// # Panics
    /// Upstream panics if subclass installation fails or the window is visible.
    pub unsafe fn new(
        window: W,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, HandleError> {
        let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
            return Err(HandleError::NotSupported);
        };
        let (mailbox, handlers) = Mailbox::new(wake);
        let native = SubclassingAdapter::new(
            HWND(handle.hwnd.get() as *mut _),
            handlers.clone(),
            handlers,
        );
        Ok(Self {
            native,
            mailbox,
            _window: window,
        })
    }

    /// Takes queued activation or action work for the UI thread.
    pub fn next_event(&mut self) -> Option<Event> {
        self.mailbox.next_event()
    }

    /// Publishes a full initial tree or a delta only while UIA is active.
    /// Native notifications are raised after releasing AccessKit's tree borrow;
    /// reentrant native queries never borrow the application's retained tree.
    pub fn update_if_active(&mut self, build: impl FnOnce(bool) -> TreeUpdate) -> bool {
        let mut published = false;
        let events = self.native.update_if_active(|| {
            let update = build(self.mailbox.take_initial_request());
            published = true;
            update
        });
        if let Some(events) = events {
            events.raise();
        }
        published
    }
}
