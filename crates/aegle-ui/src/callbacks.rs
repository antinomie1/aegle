use crate::{Node, Result, Ui, state::State};
use aegle_core::NodeId;
use std::{collections::HashMap, rc::Rc};

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

/// One event's handlers on one node, in registration order, with the version
/// queued invocations must match.
pub struct Handler {
    pub version: u64,
    pub callbacks: Vec<Callback>,
}

/// Appends `callback` to the handlers of `id`. A first handler gets a new
/// version, unique across event kinds, which queued invocations name.
pub(crate) fn add(
    handlers: &mut HashMap<NodeId, Handler>,
    next_version: &mut u64,
    id: NodeId,
    callback: Callback,
) {
    if let Some(handler) = handlers.get_mut(&id) {
        handler.callbacks.push(callback);
        return;
    }
    *next_version += 1;
    let version = *next_version;
    let callbacks = vec![callback];
    handlers.insert(id, Handler { version, callbacks });
}

impl Node {
    /// Adds an action handler (click, change, submit...); typed handles'
    /// `on_*` methods wrap it with their own callback types. See
    /// [`State::on_action`].
    pub fn on_action(&self, callback: impl FnMut(Node) -> Result + 'static) {
        self.change(|state, id| {
            state.on_action(id, callback);
            Ok(())
        })
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
    pub fn on_action(&mut self, id: NodeId, callback: impl FnMut(Node) -> Result + 'static) {
        add(
            &mut self.callbacks,
            &mut self.callback_version,
            id,
            Box::new(callback),
        )
    }
    /// Queues the action handlers of `id`, if it has any, to run after the
    /// current input batch. Controls call it when the user activated them or
    /// changed their value, including from a handler of another control.
    pub fn queue_action(&mut self, id: NodeId) {
        if let Some(handler) = self.callbacks.get(&id) {
            self.pending.push_back((id, handler.version));
        }
    }
    /// The handler a queued version names; versions are unique across kinds.
    fn handler(&mut self, id: NodeId, version: u64) -> Option<&mut Handler> {
        let current = |h: &&mut Handler| h.version == version;
        if let Some(handler) = self.callbacks.get_mut(&id).filter(current) {
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
            let next = {
                let mut state = self.state.borrow_mut();
                let Some((id, version)) = state.pending.pop_front() else {
                    break;
                };
                state
                    .handler(id, version)
                    .map(|h| std::mem::take(&mut h.callbacks))
                    .map(|callbacks| (id, version, callbacks))
            };
            let Some((id, version, mut callbacks)) = next else {
                continue;
            };
            for callback in &mut callbacks {
                let node = Node {
                    id,
                    state: Rc::downgrade(&self.state),
                };
                if let Err(error) = callback(node)
                    && failure.is_ok()
                {
                    failure = Err(error);
                }
            }
            let mut state = self.state.borrow_mut();
            // Handlers added while these ran follow them.
            if let Some(handler) = state.handler(id, version) {
                callbacks.append(&mut handler.callbacks);
                handler.callbacks = callbacks;
            }
            state.drops.finished(id, version);
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
