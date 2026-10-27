#![allow(unsafe_code)]

use crate::{Event, Mailbox};
use accesskit::TreeUpdate;
use accesskit_macos::SubclassingAdapter;
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle};

/// Optional NSAccessibility adapter over the window's content view.
///
/// The adapter subclasses the `NSView`: VoiceOver, the rotor and the Keyboard
/// pane read the retained semantic tree through `accessibilityChildren`,
/// `accessibilityFocusedUIElement` and `accessibilityHitTest:`. Native
/// callbacks only enqueue work, so they never borrow the application's tree.
/// AccessKit answers with a placeholder until the UI thread publishes the
/// first tree.
pub struct MacosAdapter<W> {
    // Dropped before the view lease, restoring the view's original class.
    native: SubclassingAdapter,
    mailbox: Mailbox,
    _window: W,
}

impl<W: HasWindowHandle> MacosAdapter<W> {
    /// Installs the subclass before the window's first presentation.
    /// `wake` must signal the owning event loop without blocking.
    ///
    /// # Safety
    /// Call on the main thread while the view is still hidden and not yet
    /// focused, before anything observes it with key-value observing. The
    /// supplied owner must keep that same view alive until this adapter is
    /// dropped, and nothing else may install an accessibility subclass on it.
    ///
    /// # Panics
    /// Upstream panics if the view already has an adapter installed.
    pub unsafe fn new(
        window: W,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, HandleError> {
        let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
            return Err(HandleError::NotSupported);
        };
        let (mailbox, handlers) = Mailbox::new(wake);
        // SAFETY: the caller keeps the view alive and untouched until drop.
        let native =
            unsafe { SubclassingAdapter::new(handle.ns_view.as_ptr(), handlers.clone(), handlers) };
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

    /// Publishes a full initial tree or a delta only while accessibility is
    /// active. Events are raised after AccessKit's tree borrow ends, so a
    /// reentrant native query never borrows the application's retained tree.
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

    /// Reports whether the window itself is focused, apart from which control
    /// holds the application's logical focus.
    pub fn set_window_focused(&mut self, focused: bool) {
        if let Some(events) = self.native.update_view_focus_state(focused) {
            events.raise();
        }
    }
}
