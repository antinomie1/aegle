use crate::{Event, Mailbox};
use accesskit::TreeUpdate;

/// Opt-in AT-SPI adapter, usable with Wayland or another UI host.
///
/// AccessKit keeps its own semantic cache and starts a process-wide background
/// thread on first construction. That thread persists after adapters are dropped.
/// The upstream adapter currently has no bus-error/status API or EditableText
/// interface; construction is not proof that a screen reader is connected.
pub struct UnixAdapter {
    native: accesskit_unix::Adapter,
    mailbox: Mailbox,
}

impl UnixAdapter {
    /// Connects callbacks to a nonblocking event-loop wake function.
    /// Actual AT-SPI activation is asynchronous; no UI tree is built here.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (mailbox, handlers) = Mailbox::new(wake);
        let native = accesskit_unix::Adapter::new(handlers.clone(), handlers.clone(), handlers);
        Self { native, mailbox }
    }

    /// Takes queued work for the owning UI thread.
    pub fn next_event(&mut self) -> Option<Event> {
        self.mailbox.next_event()
    }

    /// Publishes only when native accessibility is active or pending activation.
    /// `initial` requests a complete tree; otherwise a delta is sufficient.
    /// The factory executes synchronously on the caller's thread, without a
    /// second host-side semantic cache. Returns whether the factory was called.
    /// Every update must follow AccessKit's identity and tree consistency rules.
    pub fn update_if_active(&mut self, build: impl FnOnce(bool) -> TreeUpdate) -> bool {
        let mut published = false;
        self.native.update_if_active(|| {
            let initial = self.mailbox.take_initial_request();
            let update = build(initial);
            published = true;
            update
        });
        published
    }

    /// Synchronizes actual native window activation separately from logical
    /// control focus. Does not attempt to activate a Wayland surface.
    pub fn set_window_focused(&mut self, focused: bool) {
        self.native.update_window_focus_state(focused);
    }
}
