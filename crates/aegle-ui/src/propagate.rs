//! Capture and bubble listeners. Pointer, key and wheel input travels from the
//! root down to the node it is routed to and back up, through the listeners
//! on that path, before the engine and the target control handle it.

use std::{collections::HashMap, rc::Rc};

use aegle_controls::{Modifiers, PointerId, PointerKind};
use aegle_core::NodeId;
use aegle_types::Point;

use crate::{
    HandlerResult, KeyEvent, Node, Result, Subscription, Ui, UiError,
    callbacks::{self, Callbacks, Slot},
};

/// Where an [`InputEvent`] is on its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// At an ancestor of the target, going down from the root.
    Capture,
    /// At the target itself.
    Target,
    /// At an ancestor of the target, going back up to the root.
    Bubble,
}

/// The input an [`InputEvent`] carries.
#[derive(Clone, Copy, Debug)]
pub enum InputKind<'a> {
    /// A pointer transition in logical window coordinates, with the press
    /// count already counted.
    Pointer {
        /// The pointer.
        pointer: PointerId,
        /// What it did.
        kind: PointerKind,
        /// Where, in logical window coordinates.
        position: Point,
        /// Modifiers held.
        modifiers: Modifiers,
    },
    /// A key, after the window key handlers ([`Ui::on_key`]) passed it on.
    Key(KeyEvent<'a>),
    /// Wheel or touchpad scrolling at a point.
    Wheel {
        /// Where, in logical window coordinates.
        position: Point,
        /// The logical displacement.
        delta: Point,
        /// Modifiers held.
        modifiers: Modifiers,
    },
}

/// One input on its way to a target, given to listeners in turn.
pub struct InputEvent<'a> {
    /// What happened.
    pub kind: InputKind<'a>,
    /// The node the input is routed to: the pointer's capture or the topmost
    /// node under it, the focused control (else the root) for keys. It may be
    /// removed by an earlier listener; check `is_alive` before using it.
    pub target: Node,
    phase: Phase,
    stopped: bool,
    prevented: bool,
}

impl InputEvent<'_> {
    /// Where on the path this listener runs.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// Stops the path after the listeners of the current node.
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }
    /// Skips what the engine and controls would do with the input: routing a
    /// pointer to its control (hover still follows it), double clicks and
    /// context menus; key bindings, focus traversal and the focused control;
    /// scrolling. The rest of the path still runs.
    pub fn prevent_default(&mut self) {
        self.prevented = true;
    }
    /// Whether a listener called [`Self::prevent_default`].
    pub fn default_prevented(&self) -> bool {
        self.prevented
    }
}

type Listener = Box<dyn FnMut(Node, &mut InputEvent<'_>) -> Result>;

/// Capture and bubble listeners per node.
#[derive(Default)]
pub struct Listeners {
    nodes: HashMap<NodeId, [Callbacks<Listener>; 2]>,
}

impl Listeners {
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub(crate) fn unregister(&mut self, id: NodeId, serial: u64) {
        if let Some(lists) = self.nodes.get_mut(&id) {
            lists
                .iter_mut()
                .for_each(|l| l.retain(|(s, _)| *s != serial));
            if lists.iter().all(Vec::is_empty) {
                self.nodes.remove(&id);
            }
        }
    }
    pub(crate) fn forget(&mut self, id: NodeId) {
        self.nodes.remove(&id);
    }
    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
    }
}

impl Node {
    /// Adds a listener for pointer, key and wheel input routed to this node
    /// or a descendant, run as the input bubbles back up from its target,
    /// before the target control handles it. Listeners run outside every UI
    /// borrow, in registration order; one added during dispatch runs from the
    /// next input. An error is returned by the input call after the default
    /// behavior, which it does not prevent.
    pub fn on_input<R: HandlerResult>(
        &self,
        listener: impl FnMut(Node, &mut InputEvent<'_>) -> R + 'static,
    ) -> Subscription {
        self.listen(false, listener)
    }
    /// Adds a listener like [`Self::on_input`] that runs as the input goes
    /// down from the root to its target, before any bubbling listener.
    pub fn on_input_capture<R: HandlerResult>(
        &self,
        listener: impl FnMut(Node, &mut InputEvent<'_>) -> R + 'static,
    ) -> Subscription {
        self.listen(true, listener)
    }
    fn listen<R: HandlerResult>(
        &self,
        capture: bool,
        mut listener: impl FnMut(Node, &mut InputEvent<'_>) -> R + 'static,
    ) -> Subscription {
        self.change(|state, id| {
            let serial = callbacks::serial(&mut state.callback_version);
            let listener: Listener =
                Box::new(move |node, event| listener(node, event).into_result());
            let lists = state.listeners.nodes.entry(id).or_default();
            lists[usize::from(!capture)].push((serial, Some(listener)));
            Ok(state.subscription(Slot::Input(id), serial))
        })
    }
}

impl Ui {
    /// Delivers a pointer transition down and up the path to the node it is
    /// routed to, then, unless a listener prevented it, to its control.
    pub(crate) fn dispatch_pointer(
        &self,
        id: PointerId,
        kind: PointerKind,
        position: Point,
        modifiers: Modifiers,
    ) -> Result {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(UiError::InvalidValue.into());
        }
        let live = !matches!(kind, PointerKind::Leave | PointerKind::Cancel);
        let (kind, target) = {
            let mut state = self.write();
            state.rebuild_order();
            let kind = match kind {
                PointerKind::Down { clicks } => PointerKind::Down {
                    clicks: state.count_click(position, clicks),
                },
                kind => kind,
            };
            // The capture, else the topmost node under the pointer.
            let captured = state.capture.filter(|(pointer, _)| *pointer == id);
            let under = || live.then(|| state.node_at(position, false)).flatten();
            let routed = || captured.map(|(_, node)| node).or_else(under);
            (kind, (!state.listeners.is_empty()).then(routed).flatten())
        };
        let event = crate::InputKind::Pointer {
            pointer: id,
            kind,
            position,
            modifiers,
        };
        let (prevented, failure) = self.propagate(target, event);
        if prevented {
            let mut state = self.write();
            state.rebuild_order();
            state.pointer = live.then_some((id, position));
            let hit = state.pointer.and_then(|_| state.hit(position));
            state.update_hover(hit, id, position)?;
            return failure;
        }
        self.route_pointer(id, kind, position, modifiers)?;
        failure
    }
    /// Runs the listeners on the path to `target`, capturing then bubbling.
    /// Returns whether one prevented the default, and the first error.
    pub(crate) fn propagate(&self, target: Option<NodeId>, kind: InputKind<'_>) -> (bool, Result) {
        let (path, until) = {
            let state = self.read();
            let Some(target) = target.filter(|_| !state.listeners.is_empty()) else {
                return (false, Ok(()));
            };
            let mut path = Vec::new();
            let mut node = Some(target);
            while let Some(id) = node {
                if state.listeners.nodes.contains_key(&id) {
                    path.push(id);
                }
                node = state.tree.parent(id).ok().flatten();
            }
            (path, state.callback_version)
        };
        let Some(target) = target else {
            return (false, Ok(()));
        };
        let mut event = InputEvent {
            kind,
            target: self.node(target),
            phase: Phase::Capture,
            stopped: false,
            prevented: false,
        };
        let mut failure = Ok(());
        let phase = |id, outer| if id == target { Phase::Target } else { outer };
        for &id in path.iter().rev() {
            event.phase = phase(id, Phase::Capture);
            self.run_listeners(id, 0, &mut event, until, &mut failure);
            if event.stopped {
                return (event.prevented, failure);
            }
        }
        for &id in &path {
            event.phase = phase(id, Phase::Bubble);
            self.run_listeners(id, 1, &mut event, until, &mut failure);
            if event.stopped {
                break;
            }
        }
        (event.prevented, failure)
    }

    fn run_listeners(
        &self,
        id: NodeId,
        list: usize,
        event: &mut InputEvent<'_>,
        until: u64,
        failure: &mut Result,
    ) {
        let mut after = 0;
        loop {
            let next = {
                let mut state = self.state.borrow_mut();
                let lists = state.listeners.nodes.get_mut(&id);
                lists.and_then(|lists| callbacks::take(&mut lists[list], after, until))
            };
            let Some((serial, mut listener)) = next else {
                return;
            };
            after = serial;
            if let Err(error) = listener(self.node(id), event)
                && failure.is_ok()
            {
                *failure = Err(error);
            }
            if let Some(lists) = self.state.borrow_mut().listeners.nodes.get_mut(&id) {
                callbacks::restore(&mut lists[list], serial, listener);
            }
        }
    }

    fn node(&self, id: NodeId) -> Node {
        Node {
            state: Rc::downgrade(&self.state),
            id,
        }
    }
}
