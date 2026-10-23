use crate::{Node, Result, Ui, UiError, handles::fail, state::State};
use aegle_core::{Dirty, NodeId};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

/// What an event handler returns: nothing, or a [`Result`] whose error the
/// host reports (the native App passes it to `App::on_error`).
pub trait HandlerResult {
    /// The handler's outcome as a `Result`.
    fn into_result(self) -> Result;
}

impl HandlerResult for () {
    fn into_result(self) -> Result {
        Ok(())
    }
}

impl HandlerResult for Result {
    fn into_result(self) -> Result {
        self
    }
}

/// A boxed application callback taking the control it was registered on.
pub type Callback = Box<dyn FnMut(Node) -> Result>;

/// Registered callbacks by serial, in registration order; a callback's slot
/// is empty while it runs.
pub type Callbacks<C> = Vec<(u64, Option<C>)>;

/// One event's handlers on one node, in registration order, with the version
/// queued invocations must match.
pub struct Handler {
    pub version: u64,
    pub callbacks: Callbacks<Callback>,
}

/// Where a [`Subscription`]'s handler is registered.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Slot {
    Action(NodeId),
    Edit(NodeId),
    #[cfg(feature = "motion")]
    TransitionEnd(NodeId),
    DoubleClick(NodeId),
    ContextMenu(NodeId),
    Drop(NodeId),
    Frame,
    Key,
    Input(NodeId),
}

/// A registered handler. Every `on_*` method returns one; dropping it keeps
/// the handler, which otherwise lives as long as its control.
#[derive(Clone)]
pub struct Subscription {
    pub(crate) state: Weak<RefCell<State>>,
    pub(crate) slot: Slot,
    pub(crate) serial: u64,
}

impl Subscription {
    /// Unregisters the handler: it never runs again, even for an event
    /// already being dispatched. Removing it twice, or after its control or
    /// window is gone, does nothing.
    ///
    /// # Panics
    ///
    /// Like handle methods, when called from a painter, hook or scene visitor.
    pub fn remove(&self) {
        let Some(owner) = self.state.upgrade() else {
            return;
        };
        let mut state = owner
            .try_borrow_mut()
            .unwrap_or_else(|_| fail(UiError::ReentrantAccess.into()));
        state.unsubscribe(self.slot, self.serial);
    }
}

/// The next registration serial, unique across every kind of handler.
pub(crate) fn serial(next: &mut u64) -> u64 {
    *next += 1;
    *next
}

/// Appends `callback` to the handlers of `id`, returning its serial. A first
/// handler's serial is also the version queued invocations name.
pub(crate) fn add(
    handlers: &mut HashMap<NodeId, Handler>,
    next: &mut u64,
    id: NodeId,
    callback: Callback,
) -> u64 {
    let serial = serial(next);
    handlers
        .entry(id)
        .or_insert_with(|| Handler {
            version: serial,
            callbacks: Vec::new(),
        })
        .callbacks
        .push((serial, Some(callback)));
    serial
}

/// Takes the first callback registered after `after` and by `until`, so a
/// dispatch runs each once and skips those added meanwhile.
pub(crate) fn take<C>(list: &mut Callbacks<C>, after: u64, until: u64) -> Option<(u64, C)> {
    let (serial, slot) = list
        .iter_mut()
        .find(|(serial, slot)| *serial > after && *serial <= until && slot.is_some())?;
    Some((*serial, slot.take().unwrap()))
}

/// Puts a callback back after it ran, unless it was removed meanwhile.
pub(crate) fn restore<C>(list: &mut Callbacks<C>, serial: u64, callback: C) {
    if let Some((_, slot)) = list.iter_mut().find(|(s, _)| *s == serial) {
        *slot = Some(callback);
    }
}

/// Removes the callback `serial` of `id`, dropping its handler once empty.
fn unregister(handlers: &mut HashMap<NodeId, Handler>, id: NodeId, serial: u64) {
    if let Some(handler) = handlers.get_mut(&id) {
        handler.callbacks.retain(|(s, _)| *s != serial);
        if handler.callbacks.is_empty() {
            handlers.remove(&id);
        }
    }
}

impl Node {
    /// Adds an action handler (click, change, submit...); typed handles'
    /// `on_*` methods wrap it with their own callback types. See
    /// [`State::on_action`].
    pub fn on_action(&self, callback: impl FnMut(Node) -> Result + 'static) -> Subscription {
        self.change(|state, id| Ok(state.on_action(id, callback)))
    }
    /// Keeps `value` until this control is removed or its window closes, tying
    /// application state such as markup bindings to the control's lifetime.
    /// The value is dropped while the UI is being modified, so its `Drop` must
    /// not use this UI.
    pub fn keep_alive(&self, value: impl std::any::Any) {
        self.change(|state, id| {
            state.kept.entry(id).or_default().push(Box::new(value));
            Ok(())
        })
    }
}

impl State {
    /// Adds an action handler (click, change, submit...) to `id`; typed
    /// handles wrap it with their own callback types. Handlers run in
    /// registration order after the input batch, outside every UI borrow, so
    /// they may create or remove controls.
    pub fn on_action(
        &mut self,
        id: NodeId,
        callback: impl FnMut(Node) -> Result + 'static,
    ) -> Subscription {
        let callback = Box::new(callback);
        let serial = add(
            &mut self.callbacks,
            &mut self.callback_version,
            id,
            callback,
        );
        self.subscription(Slot::Action(id), serial)
    }
    /// Adds a handler to `id` run after user input changed the committed text
    /// of its editor: typing, deletion, paste, cut, undo, redo or an IME
    /// commit, but not preedit or programmatic text. It runs like an action
    /// handler, once per changing input.
    pub fn on_edit(
        &mut self,
        id: NodeId,
        callback: impl FnMut(Node) -> Result + 'static,
    ) -> Subscription {
        let serial = add(
            &mut self.edits,
            &mut self.callback_version,
            id,
            Box::new(callback),
        );
        self.subscription(Slot::Edit(id), serial)
    }
    /// A subscription to handler `serial` in `slot`.
    pub(crate) fn subscription(&self, slot: Slot, serial: u64) -> Subscription {
        Subscription {
            state: self.weak.clone(),
            slot,
            serial,
        }
    }
    /// Queues the action handlers of `id`, if it has any, to run after the
    /// current input batch. Controls call it when the user activated them or
    /// changed their value, including from a handler of another control.
    /// Returns whether `id` had handlers to queue.
    pub fn queue_action(&mut self, id: NodeId) -> bool {
        let handler = self.callbacks.get(&id);
        if let Some(handler) = handler {
            self.pending.push_back((id, handler.version));
        }
        handler.is_some()
    }
    /// Queues the edit handlers of `id`, if it has any, like [`Self::queue_action`].
    pub(crate) fn queue_edit(&mut self, id: NodeId) {
        if let Some(handler) = self.edits.get(&id) {
            self.pending.push_back((id, handler.version));
        }
    }
    /// Unregisters the handler `serial` in `slot`.
    pub(crate) fn unsubscribe(&mut self, slot: Slot, serial: u64) {
        match slot {
            Slot::Action(id) => unregister(&mut self.callbacks, id, serial),
            Slot::Edit(id) => unregister(&mut self.edits, id, serial),
            #[cfg(feature = "motion")]
            Slot::TransitionEnd(id) => unregister(&mut self.motion.ends, id, serial),
            Slot::DoubleClick(id) => unregister(&mut self.clicks.handlers, id, serial),
            Slot::ContextMenu(id) => {
                unregister(&mut self.clicks.menus, id, serial);
                // Whether a context menu is offered is part of the semantics.
                if self.tree.get(id).is_some() {
                    let _ = self.tree.mark_dirty(id, Dirty::SEMANTICS);
                }
            }
            Slot::Drop(id) => unregister(&mut self.drops.handlers, id, serial),
            Slot::Frame => self.frames.retain(|h| h.version != serial),
            Slot::Key => self.key_handlers.retain(|(s, _)| *s != serial),
            Slot::Input(id) => self.listeners.unregister(id, serial),
        }
    }
    /// The handler a queued version names; versions are unique across kinds.
    fn handler(&mut self, id: NodeId, version: u64) -> Option<&mut Handler> {
        let current = |h: &&mut Handler| h.version == version;
        if let Some(handler) = self.callbacks.get_mut(&id).filter(current) {
            return Some(handler);
        }
        if let Some(handler) = self.edits.get_mut(&id).filter(current) {
            return Some(handler);
        }
        #[cfg(feature = "motion")]
        if let Some(handler) = self.motion.ends.get_mut(&id).filter(current) {
            return Some(handler);
        }
        if let Some(handler) = self.clicks.handlers.get_mut(&id).filter(current) {
            return Some(handler);
        }
        if let Some(handler) = self.drops.handlers.get_mut(&id).filter(current) {
            return Some(handler);
        }
        self.clicks.menus.get_mut(&id).filter(current)
    }
}

impl Ui {
    /// Invokes queued actions without holding the tree borrow. A callback queued
    /// by another callback waits until the next call, preventing recursive dispatch.
    /// A failing handler stays installed; the other handlers of its event still
    /// run, then the first error is returned and later invocations stay queued
    /// for the next call. Earlier valid changes remain.
    pub fn dispatch_callbacks(&self) -> Result {
        let count = {
            let mut state = self.write();
            if state.dispatching {
                return Ok(());
            }
            state.dispatching = true;
            state.pending.len()
        };
        let mut failure = Ok(());
        for _ in 0..count {
            let (id, version, until) = {
                let mut state = self.state.borrow_mut();
                let Some((id, version)) = state.pending.pop_front() else {
                    break;
                };
                (id, version, state.callback_version)
            };
            // Handlers added while these run follow them, from the next event.
            let mut after = 0;
            loop {
                let next = {
                    let mut state = self.state.borrow_mut();
                    let handler = state.handler(id, version);
                    handler.and_then(|h| take(&mut h.callbacks, after, until))
                };
                let Some((serial, mut callback)) = next else {
                    break;
                };
                after = serial;
                let node = Node {
                    id,
                    state: Rc::downgrade(&self.state),
                };
                if let Err(error) = callback(node)
                    && failure.is_ok()
                {
                    failure = Err(error);
                }
                if let Some(handler) = self.state.borrow_mut().handler(id, version) {
                    restore(&mut handler.callbacks, serial, callback);
                }
            }
            self.state.borrow_mut().drops.finished(id, version);
            if failure.is_err() {
                break;
            }
        }
        self.state.borrow_mut().dispatching = false;
        failure
    }
    /// Whether application callbacks still need a dispatch pass before sleeping.
    pub fn has_pending_callbacks(&self) -> bool {
        !self.read().pending.is_empty()
    }
}
