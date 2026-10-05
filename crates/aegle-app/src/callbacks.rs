use crate::{Button, Node, Result, TextField, Ui, UiError, state::State};
use aegle_core::NodeId;
use std::rc::Rc;

pub(crate) struct Handler {
    pub version: u64,
    pub callback: Option<Box<dyn FnMut(Node) -> Result>>,
}

impl Button {
    /// Replaces the click handler. Invocation occurs after the input batch,
    /// outside every UI/tree borrow, so handles may safely create or remove controls.
    pub fn on_click(&self, mut callback: impl FnMut(Button) -> Result + 'static) -> Result {
        self.0.on_action(move |node| callback(Button(node)))
    }
    /// Removes the click handler and invalidates any already queued invocation.
    pub fn clear_on_click(&self) -> Result {
        self.0.clear_on_action()
    }
}
impl TextField {
    /// Replaces the single-line Enter handler, dispatched outside the tree borrow.
    pub fn on_submit(&self, mut callback: impl FnMut(TextField) -> Result + 'static) -> Result {
        self.0.on_action(move |node| callback(TextField(node)))
    }
    /// Removes the submit handler and any queued invocation.
    pub fn clear_on_submit(&self) -> Result {
        self.0.clear_on_action()
    }
}
impl Node {
    pub(crate) fn on_action(&self, callback: impl FnMut(Node) -> Result + 'static) -> Result {
        self.change(|state, id| {
            state.callback_version = state
                .callback_version
                .checked_add(1)
                .ok_or(UiError::IdentityExhausted)?;
            state.callbacks.insert(
                id,
                Handler {
                    version: state.callback_version,
                    callback: Some(Box::new(callback)),
                },
            );
            Ok(())
        })
    }
    pub(crate) fn clear_on_action(&self) -> Result {
        self.change(|state, id| {
            state.callbacks.remove(&id);
            Ok(())
        })
    }
}

impl State {
    /// The current handler with a queued version; versions are unique across kinds.
    fn handler(&mut self, id: NodeId, version: u64) -> Option<&mut Handler> {
        let handler = self.callbacks.get_mut(&id);
        #[cfg(feature = "motion")]
        let handler = handler
            .filter(|h| h.version == version)
            .or(self.motion.ends.get_mut(&id));
        handler.filter(|h| h.version == version)
    }
}

impl Ui {
    /// Invokes queued actions without holding the tree borrow. A callback queued
    /// by another callback waits until the next call, preventing recursive dispatch.
    /// On failure the failing handler is removed; earlier valid changes remain.
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
        for _ in 0..count {
            let next = {
                let mut state = self.state.borrow_mut();
                let Some((id, version)) = state.pending.pop_front() else {
                    break;
                };
                state
                    .handler(id, version)
                    .and_then(|h| h.callback.take())
                    .map(|callback| (id, version, callback))
            };
            let Some((id, version, mut callback)) = next else {
                continue;
            };
            let result = callback(Node {
                id,
                state: Rc::downgrade(&self.state),
            });
            let mut state = self.state.borrow_mut();
            // A failing handler stays disabled until replaced.
            if let Some(handler) = state.handler(id, version) {
                handler.callback = result.is_ok().then_some(callback);
            }
            if let Err(error) = result {
                state.dispatching = false;
                return Err(error);
            }
        }
        self.state.borrow_mut().dispatching = false;
        Ok(())
    }
    /// Whether application callbacks still need a dispatch pass before sleeping.
    pub fn has_pending_callbacks(&self) -> bool {
        !self.state.borrow().pending.is_empty()
    }
}
