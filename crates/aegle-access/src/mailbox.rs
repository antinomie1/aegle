use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

/// Work delivered by a platform callback; process it on the UI thread.
#[derive(Debug)]
pub enum Event {
    /// Build a complete tree even when no pixels need repainting.
    InitialTree,
    /// Route through current liveness, enabled and read-only checks.
    Action(ActionRequest),
    /// Release derived data that can be rebuilt on the next activation.
    Deactivate,
}

/// UI-thread receiver for platform callbacks, independent of any event loop.
///
/// The channel preserves callback order and does not drop actions. It is
/// unbounded: hosts must drain it on every wake and keep handlers short. It
/// retains only pending events, not an additional copy of the semantic tree.
pub struct Mailbox {
    receiver: mpsc::Receiver<Event>,
    initial: Arc<AtomicBool>,
}

/// Cloneable AccessKit callback implementations. They enqueue and wake only;
/// no tree borrowing, main-thread wait, or UI callback occurs on the OS thread.
#[derive(Clone)]
pub struct Handlers {
    sender: mpsc::Sender<Event>,
    initial: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Mailbox {
    /// Creates a channel. `wake` must be nonblocking and signal the UI loop.
    /// The wake is issued after the event is stored, avoiding a lost wake race.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> (Self, Handlers) {
        let (sender, receiver) = mpsc::channel();
        let initial = Arc::new(AtomicBool::new(true));
        let handlers = Handlers {
            sender,
            initial: initial.clone(),
            wake: Arc::new(wake),
        };
        (Self { receiver, initial }, handlers)
    }

    /// Takes the oldest queued callback without blocking.
    pub fn next_event(&mut self) -> Option<Event> {
        self.receiver.try_recv().ok()
    }

    /// Takes the request for a complete initial tree. Consume this only inside
    /// an active adapter's publication callback, not while draining events.
    /// This also covers an activation arriving before its queued wake is drained.
    pub fn take_initial_request(&self) -> bool {
        self.initial.swap(false, Ordering::AcqRel)
    }
}

impl Handlers {
    fn send(&self, event: Event) {
        if self.sender.send(event).is_ok() {
            (self.wake)();
        }
    }
}

impl ActivationHandler for Handlers {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.initial.store(true, Ordering::Release);
        self.send(Event::InitialTree);
        None
    }
}
impl ActionHandler for Handlers {
    fn do_action(&mut self, request: ActionRequest) {
        self.send(Event::Action(request));
    }
}
impl DeactivationHandler for Handlers {
    fn deactivate_accessibility(&mut self) {
        self.initial.store(true, Ordering::Release);
        self.send(Event::Deactivate);
    }
}
