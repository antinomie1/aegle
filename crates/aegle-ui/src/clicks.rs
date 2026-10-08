//! Multi-click counting for every host, and double-click handlers on any node.

use std::{collections::HashMap, time::Duration, time::Instant};

use aegle_core::NodeId;
use aegle_types::Point;

use crate::{Node, Result, Ui, UiError, callbacks::Handler, state::State};

/// The last primary press and the rule for counting the next one.
pub struct Clicks {
    last: Option<(Instant, Point, u8)>,
    interval: Duration,
    distance: f32,
    /// Double-click handlers, versioned in the shared callback sequence.
    pub handlers: HashMap<NodeId, Handler>,
}

impl Default for Clicks {
    fn default() -> Self {
        Self {
            last: None,
            interval: Duration::from_millis(400),
            distance: 4.0,
            handlers: HashMap::new(),
        }
    }
}

impl State {
    /// Counts a primary press at the input time: within the interval and
    /// distance of the previous press it continues 1, 2, 3, then starts
    /// over. A host's own count is a lower bound.
    pub(crate) fn count_click(&mut self, position: Point, reported: u8) -> u8 {
        let clicks = &mut self.clicks;
        let counted = match clicks.last {
            Some((at, from, count))
                if self.input_time.saturating_duration_since(at) <= clicks.interval
                    && (position.x - from.x).abs() <= clicks.distance
                    && (position.y - from.y).abs() <= clicks.distance =>
            {
                count % 3 + 1
            }
            _ => 1,
        };
        let count = counted.max(reported);
        clicks.last = Some((self.input_time, position, count));
        count
    }

    /// Queues the double-click handler of `hit` or its nearest ancestor that has one.
    pub(crate) fn double_click(&mut self, hit: NodeId) {
        let mut node = Some(hit);
        while let Some(id) = node {
            if let Some(handler) = self.clicks.handlers.get(&id) {
                self.pending.push_back((id, handler.version));
                return;
            }
            node = self.tree.parent(id).ok().flatten();
        }
    }
}

impl Ui {
    /// Sets how close in time and logical pixels presses must be to count as
    /// a double or triple click; hosts pass the system setting. The default
    /// is 400 ms and 4 px.
    pub fn set_double_click(&self, interval: Duration, distance: f32) -> Result {
        crate::valid(distance)?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.clicks.interval = interval;
        state.clicks.distance = distance;
        Ok(())
    }
}

impl Node {
    /// Adds a handler run when the primary button is double-clicked over
    /// this control or a descendant without its own double-click handler,
    /// after the press's normal behavior. Handlers run like click handlers,
    /// outside every UI borrow, in registration order.
    pub fn on_double_click(&self, callback: impl FnMut(Node) -> Result + 'static) -> Result {
        self.change(|state, id| {
            let callback = Box::new(callback);
            let version = &mut state.callback_version;
            crate::callbacks::add(&mut state.clicks.handlers, version, id, callback)
        })
    }
    /// Removes the double-click handlers.
    pub fn clear_on_double_click(&self) -> Result {
        self.change(|state, id| {
            state.clicks.handlers.remove(&id);
            Ok(())
        })
    }
}
