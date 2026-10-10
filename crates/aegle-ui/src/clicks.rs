//! Multi-click counting for every host, and double-click and context-menu
//! handlers on any node.

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
    /// Context-menu handlers, versioned the same way.
    pub menus: HashMap<NodeId, Handler>,
    /// Window point of the latest context-menu request.
    pub menu_at: Point,
}

impl Default for Clicks {
    fn default() -> Self {
        Self {
            last: None,
            interval: Duration::from_millis(400),
            distance: 4.0,
            handlers: HashMap::new(),
            menus: HashMap::new(),
            menu_at: Point::new(0.0, 0.0),
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
        if let Some(id) = self.handling(hit, |clicks| &clicks.handlers) {
            self.pending
                .push_back((id, self.clicks.handlers[&id].version));
        }
    }

    /// Queues the context-menu handler of `target` or its nearest ancestor
    /// that has one, to show at window point `at`; returns whether one exists.
    pub(crate) fn context_menu(&mut self, target: NodeId, at: Point) -> bool {
        let Some(id) = self.handling(target, |clicks| &clicks.menus) else {
            return false;
        };
        self.clicks.menu_at = at;
        self.pending.push_back((id, self.clicks.menus[&id].version));
        true
    }

    /// Asks for the context menu of the focused control, or of the root
    /// without focus, at the control's top-left corner.
    pub(crate) fn keyboard_context_menu(&mut self) -> bool {
        let target = self.focus.current(&self.tree).unwrap_or(self.root);
        let at = self.tree.get(target).unwrap().context.bounds.origin;
        self.context_menu(target, at)
    }

    fn handling(
        &self,
        from: NodeId,
        handlers: impl Fn(&Clicks) -> &HashMap<NodeId, Handler>,
    ) -> Option<NodeId> {
        let mut node = Some(from);
        while let Some(id) = node {
            if handlers(&self.clicks).contains_key(&id) {
                return Some(id);
            }
            node = self.tree.parent(id).ok().flatten();
        }
        None
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
    /// Adds a handler run when a context menu is requested over this control
    /// or a descendant without its own handler: a secondary-button press
    /// (receiving the press point), or the Menu key, Shift+F10 or the
    /// accessibility ShowContextMenu action while it or a descendant has
    /// focus (receiving the focused control's top-left corner). The point is
    /// in logical window coordinates, ready for `Menu::show_at` in
    /// `aegle-widgets`. Handlers run like click handlers.
    pub fn on_context_menu(
        &self,
        mut callback: impl FnMut(Node, Point) -> Result + 'static,
    ) -> Result {
        self.change(|state, id| {
            let callback = Box::new(move |node: Node| {
                let at = node.change(|state, _| Ok(state.clicks.menu_at))?;
                callback(node, at)
            });
            let version = &mut state.callback_version;
            crate::callbacks::add(&mut state.clicks.menus, version, id, callback)?;
            state.tree.mark_dirty(id, aegle_core::Dirty::SEMANTICS)?;
            Ok(())
        })
    }
}
