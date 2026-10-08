use crate::{Node, Result, Ui, UiError, state::State};
use aegle_core::NodeId;
use std::{collections::HashMap, rc::Rc};

/// A boxed application callback taking the control it was registered on.
pub type Callback = Box<dyn FnMut(Node) -> Result>;

/// One event's handlers on one node, in registration order, with the version
/// queued invocations must match.
pub struct Handler {
    pub version: u64,
    pub callbacks: Vec<Callback>,
}

/// Appends `callback` to the handlers of `id`, giving a first handler a new
/// version so invocations queued before a clear never reach it.
pub(crate) fn add(
    handlers: &mut HashMap<NodeId, Handler>,
    next_version: &mut u64,
    id: NodeId,
    callback: Callback,
) -> Result {
    if let Some(handler) = handlers.get_mut(&id) {
        handler.callbacks.push(callback);
        return Ok(());
    }
    *next_version = next_version
        .checked_add(1)
        .ok_or(UiError::IdentityExhausted)?;
    let version = *next_version;
    let callbacks = vec![callback];
    handlers.insert(id, Handler { version, callbacks });
    Ok(())
}

impl Node {
    /// Keeps `value` until this control is removed or its window closes, tying
    /// application state such as markup bindings to the control's lifetime.
    /// The value is dropped while the UI is being modified, so its `Drop` must
    /// not use this UI.
    pub fn keep_alive(&self, value: impl std::any::Any) -> Result {
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
    ) -> Result {
        add(
            &mut self.callbacks,
            &mut self.callback_version,
            id,
            Box::new(callback),
        )
    }
    /// Removes every action handler of `id` and invalidates queued invocations.
    pub fn clear_actions(&mut self, id: NodeId) {
        self.callbacks.remove(&id);
    }
    /// The current handler with a queued version; versions are unique across kinds.
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
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| UiError::ReentrantAccess)?;
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
            if failure.is_err() {
                break;
            }
        }
        self.state.borrow_mut().dispatching = false;
        failure
    }
    /// Whether application callbacks still need a dispatch pass before sleeping.
    pub fn has_pending_callbacks(&self) -> Result<bool> {
        Ok(!self.read()?.pending.is_empty())
    }
}
