//! Per-frame callbacks, the window key handler and input timestamps.

use std::time::Instant;

use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
use aegle_core::NodeId;
use aegle_types::Point;

use crate::{Node, Result, Ui, UiError};

/// A per-frame callback with the version it was registered under.
pub struct FrameHandler {
    pub(crate) id: NodeId,
    pub(crate) version: u64,
    pub(crate) callback: Option<Box<dyn FnMut(Node, Instant) -> Result>>,
}

/// A key press or release offered to the window before the focused control.
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent<'a> {
    /// Logical key.
    pub key: Key,
    /// Translated text; empty for non-text keys.
    pub text: &'a str,
    /// Modifiers at the event.
    pub modifiers: Modifiers,
    /// Press versus release.
    pub pressed: bool,
    /// A held-key repeat.
    pub repeat: bool,
    /// When the platform reported the key, mapped onto [`Instant`]; the
    /// delivery time if the platform gives none.
    pub time: Instant,
    /// Whether focus is in a text editor, where plain keys usually type text.
    pub editing: bool,
}

/// A window key handler; returning true consumes the key.
pub(crate) type KeyHandler = Box<dyn FnMut(KeyEvent<'_>) -> Result<bool>>;

impl Node {
    /// Runs `callback` once per presented frame, with the frame's time, until
    /// it is cleared or this control is removed. While any frame callback is
    /// registered the host keeps producing frames, paced by the display, so
    /// use it for playheads and other continuously moving content and clear it
    /// when idle. It runs before layout and painting, outside every UI borrow.
    pub fn on_frame(&self, callback: impl FnMut(Node, Instant) -> Result + 'static) -> Result {
        self.change(|state, id| {
            state.callback_version = state
                .callback_version
                .checked_add(1)
                .ok_or(UiError::IdentityExhausted)?;
            let handler = FrameHandler {
                id,
                version: state.callback_version,
                callback: Some(Box::new(callback)),
            };
            match state.frames.iter_mut().find(|h| h.id == id) {
                Some(slot) => *slot = handler,
                None => state.frames.push(handler),
            }
            // Starts the frame cycle on an otherwise idle window.
            state.repaint = true;
            Ok(())
        })
    }
    /// Stops this control's frame callback.
    pub fn clear_on_frame(&self) -> Result {
        self.change(|state, id| {
            state.frames.retain(|h| h.id != id);
            Ok(())
        })
    }
}

impl Ui {
    /// Whether a frame callback is registered or a control is animating; a
    /// host keeps requesting frames, calling [`Self::run_frame`], while true.
    pub fn wants_frames(&self) -> bool {
        let state = self.state.borrow();
        !state.animated.is_empty() || state.frames.iter().any(|h| h.callback.is_some())
    }
    /// Starts a frame at `now`: animating controls repaint at this time, then
    /// the frame callbacks registered before this call run in registration
    /// order, outside the UI borrow. Hosts call it once per frame before
    /// refreshing. A failing callback is removed and its error returned.
    pub fn run_frame(&self, now: Instant) -> Result {
        let queued: Vec<(NodeId, u64)> = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| UiError::ReentrantAccess)?;
            state.frame_time = now;
            let animated: Vec<NodeId> = state.animated.iter().copied().collect();
            for id in animated {
                state.tree.mark_dirty(id, aegle_core::Dirty::PAINT)?;
            }
            state.frames.iter().map(|h| (h.id, h.version)).collect()
        };
        for (id, version) in queued {
            let callback = {
                let mut state = self.state.borrow_mut();
                let handler = state
                    .frames
                    .iter_mut()
                    .find(|h| h.id == id && h.version == version);
                match handler.and_then(|h| h.callback.take()) {
                    Some(callback) => callback,
                    None => continue,
                }
            };
            let mut callback = callback;
            let node = Node {
                state: std::rc::Rc::downgrade(&self.state),
                id,
            };
            let result = callback(node, now);
            let mut state = self.state.borrow_mut();
            let position = state
                .frames
                .iter()
                .position(|h| h.id == id && h.version == version);
            match (position, &result) {
                (Some(index), Err(_)) => {
                    state.frames.remove(index);
                }
                (Some(index), Ok(())) => state.frames[index].callback = Some(callback),
                (None, _) => {}
            }
            result?;
        }
        Ok(())
    }
    /// When delayed control-library work (a tooltip) is due; a host waits at
    /// most until then and calls [`Self::wake`].
    pub fn next_wake(&self) -> Option<Instant> {
        self.state.borrow().wake
    }
    /// Runs delayed control-library work that is due at `now`.
    pub fn wake(&self, now: Instant) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if state.wake.is_none_or(|wake| wake > now) {
            return Ok(());
        }
        state.wake = None;
        for hook in state.hooks.clone() {
            if let Some(wake) = hook.wake {
                wake(&mut state, now)?;
            }
        }
        Ok(())
    }
    /// Installs the window key handler, replacing any previous one. It sees
    /// every key before the focused control and Tab traversal, outside the UI
    /// borrow; returning true consumes the key. Check [`KeyEvent::editing`]
    /// before taking plain keys a text field would type.
    pub fn on_key(&self, handler: impl FnMut(KeyEvent<'_>) -> Result<bool> + 'static) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.key_version += 1;
        state.key_handler = Some(Box::new(handler));
        Ok(())
    }
    /// Removes the window key handler.
    pub fn clear_on_key(&self) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.key_version += 1;
        state.key_handler = None;
        Ok(())
    }
    /// Delivers a key that the platform reported at `time`: first to the
    /// window key handler, then like [`Ui::key`].
    pub fn key_at(&self, key: KeyInput<'_>, time: Instant) -> Result {
        let (handler, version, editing) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| UiError::ReentrantAccess)?;
            state.input_time = time;
            let editing = state.focus.current(&state.tree).is_some_and(|id| {
                state
                    .tree
                    .get(id)
                    .unwrap()
                    .context
                    .control
                    .editor()
                    .is_some()
            });
            (state.key_handler.take(), state.key_version, editing)
        };
        if let Some(mut handler) = handler {
            let event = KeyEvent {
                key: key.key,
                text: key.text,
                modifiers: key.modifiers,
                pressed: key.pressed,
                repeat: key.repeat,
                time,
                editing,
            };
            let result = handler(event);
            {
                let mut state = self.state.borrow_mut();
                if state.key_version == version && result.is_ok() {
                    state.key_handler = Some(handler);
                }
            }
            if result? {
                return Ok(());
            }
        }
        self.dispatch_key(key)
    }
    /// Delivers a pointer transition that the platform reported at `time`, like [`Ui::pointer`].
    pub fn pointer_at(
        &self,
        id: PointerId,
        kind: PointerKind,
        position: Point,
        modifiers: Modifiers,
        time: Instant,
    ) -> Result {
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .input_time = time;
        self.dispatch_pointer(id, kind, position, modifiers)
    }
}
