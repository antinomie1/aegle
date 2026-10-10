//! Drag and drop: drop targets on any node, and drags a control starts.
//!
//! The host reports a native drag's motion, departure and drop; the engine
//! finds the nearest node with a drop handler under the point and queues
//! its handlers like click handlers. A control starts a drag with
//! [`Node::start_drag`]; the host takes the data after the input batch and
//! begins the native drag.

use std::collections::{HashMap, VecDeque};

use aegle_controls::Input;
use aegle_core::NodeId;
use aegle_types::{DragData, Point};

use crate::{Node, Ui, callbacks::Handler, state::State};

/// What a drop handler is told.
#[derive(Clone, Debug, PartialEq)]
pub enum DropEvent {
    /// A drag entered the control or a descendant without its own handler.
    Enter,
    /// The drag left, ended elsewhere or was cancelled.
    Leave,
    /// Data was dropped; this also ends the drag over the control.
    Drop {
        /// The dropped data.
        data: DragData,
        /// The drop point in logical window coordinates.
        position: Point,
    },
}

/// Drop handlers, the hovered target and a drag waiting for the host.
#[derive(Default)]
pub struct Drops {
    /// Drop handlers, versioned in the shared callback sequence.
    pub handlers: HashMap<NodeId, Handler>,
    /// Queued events per node, one per queued handler invocation.
    events: HashMap<NodeId, VecDeque<DropEvent>>,
    /// The node the current drag is over.
    target: Option<NodeId>,
    /// Data a control asked to drag.
    start: Option<DragData>,
}

impl Drops {
    pub(crate) fn forget(&mut self, id: NodeId) {
        self.handlers.remove(&id);
        self.events.remove(&id);
        if self.target == Some(id) {
            self.target = None;
        }
    }

    /// Drops the event of a finished invocation of a drop handler.
    pub(crate) fn finished(&mut self, id: NodeId, version: u64) {
        if self.handlers.get(&id).is_some_and(|h| h.version == version)
            && let Some(events) = self.events.get_mut(&id)
        {
            events.pop_front();
        }
    }
}

impl State {
    fn queue_drop(&mut self, id: NodeId, event: DropEvent) {
        self.drops.events.entry(id).or_default().push_back(event);
        self.pending
            .push_back((id, self.drops.handlers[&id].version));
    }

    /// The nearest node with a drop handler at `position`.
    fn drop_target(&mut self, position: Point) -> Option<NodeId> {
        self.rebuild_order();
        let mut node = self.node_at(position, false);
        while let Some(id) = node {
            if self.drops.handlers.contains_key(&id) {
                return Some(id);
            }
            node = self.tree.parent(id).ok().flatten();
        }
        None
    }

    fn retarget(&mut self, target: Option<NodeId>) {
        if self.drops.target == target {
            return;
        }
        if let Some(old) = std::mem::replace(&mut self.drops.target, target) {
            self.queue_drop(old, DropEvent::Leave);
        }
        if let Some(new) = target {
            self.queue_drop(new, DropEvent::Enter);
        }
    }
}

impl Ui {
    /// A native drag moved over the window at `position`, in logical window
    /// coordinates. Returns whether a control there takes drops; hosts
    /// accept or refuse the drag with it.
    pub fn drag_motion(&self, position: Point) -> bool {
        crate::require(position.x.is_finite() && position.y.is_finite());
        let mut state = self.write();
        let target = state.drop_target(position);
        state.retarget(target);
        target.is_some()
    }

    /// The native drag left the window or was cancelled.
    pub fn drag_leave(&self) {
        self.write().retarget(None);
    }

    /// Delivers dropped data to the control at `position`; returns whether
    /// one took it.
    pub fn drop_data(&self, position: Point, data: DragData) -> bool {
        crate::require(position.x.is_finite() && position.y.is_finite());
        let mut state = self.write();
        let target = state.drop_target(position);
        state.retarget(target);
        if let Some(id) = state.drops.target.take() {
            state.queue_drop(id, DropEvent::Drop { data, position });
        }
        target.is_some()
    }

    /// Takes the data of a drag a control started, for the host to begin a
    /// native drag with the latest press.
    pub fn take_drag(&self) -> Option<DragData> {
        self.write().drops.start.take()
    }
}

impl Node {
    /// Makes this control a drop target for itself and descendants without
    /// their own handler. Handlers run like click handlers; each drag over
    /// the control gives one [`DropEvent::Enter`], then one `Leave` or
    /// `Drop`. Text and files are both offered; a handler ignores what it
    /// does not use.
    pub fn on_drop<R: crate::HandlerResult>(
        &self,
        mut callback: impl FnMut(Node, DropEvent) -> R + 'static,
    ) {
        self.change(|state, id| {
            let callback = Box::new(move |node: Node| {
                let event = node.change(|state, id| {
                    Ok(state.drops.events.get(&id).and_then(|e| e.front()).cloned())
                });
                event.map_or(Ok(()), |event| callback(node, event).into_result())
            });
            let version = &mut state.callback_version;
            crate::callbacks::add(&mut state.drops.handlers, version, id, callback);
            Ok(())
        })
    }

    /// Starts dragging `data` from the pointer press in progress, usually
    /// from a move after a press. The control's press is cancelled, as the
    /// native drag takes the pointer.
    pub fn start_drag(&self, data: DragData) {
        self.change(|state, _| {
            state.drops.start = Some(data);
            if let Some((_, id)) = state.capture.take() {
                let outcome = state.control(id, Input::Cancel)?;
                state.effects(id, outcome)?;
            }
            Ok(())
        })
    }
}
