//! The mouse cursor shape for the pointer's current position.
//!
//! The shape follows the control under the pointer, or the control that captured
//! it during a press or drag. Text fields show an I-beam while usable, scrollbars
//! and everything else show the arrow, and [`Node::set_cursor`] overrides either
//! for a control and its descendants.
use aegle_core::NodeId;
use aegle_types::{Cursor, Point};

use crate::{Node, Result, Ui, UiError, state::State};

impl State {
    /// Shape for the last pointer position; the arrow while no pointer is inside.
    pub fn cursor(&self) -> Cursor {
        let Some((pointer, position)) = self.pointer else {
            return Cursor::Default;
        };
        if self.drag.is_some() {
            return Cursor::Default;
        }
        let captured = self
            .capture
            .filter(|(owner, _)| *owner == pointer)
            .map(|(_, id)| id);
        let Some(target) = captured.or_else(|| self.node_under(position)) else {
            return Cursor::Default;
        };
        let editor = self
            .tree
            .get(target)
            .unwrap()
            .context
            .control
            .editor()
            .is_some()
            .then_some(target);
        // A shown overlay covers any scrollbar below it.
        if self.overlay_at(position).is_none() && self.scrollbar_at(position, editor).is_some() {
            return Cursor::Default;
        }
        self.cursor_of(target)
    }

    /// Topmost visible node under a point, interactive or not, so labels, images
    /// and canvases can carry a cursor too. Disabled controls still count.
    fn node_under(&self, position: Point) -> Option<NodeId> {
        let popup = self.overlay_at(position);
        self.order.iter().rev().copied().find(|&id| {
            let element = &self.tree.get(id).unwrap().context;
            popup.is_none_or(|popup| self.contains(popup, id))
                && element.effective_visible
                && element.bounds.contains(position)
                && element.clip.is_none_or(|clip| clip.contains(position))
        })
    }

    fn explicit_cursor(&self, id: NodeId) -> Option<Cursor> {
        self.decorations.get(&id).and_then(|d| d.cursor)
    }

    /// An explicit shape on the node wins, then what its content implies, then the
    /// nearest ancestor's explicit shape.
    fn cursor_of(&self, target: NodeId) -> Cursor {
        if let Some(cursor) = self.explicit_cursor(target) {
            return cursor;
        }
        if self
            .tree
            .get(target)
            .unwrap()
            .context
            .control
            .editor()
            .is_some()
            && self.usable(target)
        {
            return Cursor::Text;
        }
        let mut node = self.tree.parent(target).ok().flatten();
        while let Some(id) = node {
            if let Some(cursor) = self.explicit_cursor(id) {
                return cursor;
            }
            node = self.tree.parent(id).ok().flatten();
        }
        Cursor::Default
    }
}

impl Ui {
    /// The cursor shape to show at the last delivered pointer position. Native
    /// hosts apply it after each batch of input; embedding hosts call it after
    /// [`Self::pointer`] and [`Self::refresh`], since layout can move controls
    /// under a stationary pointer.
    pub fn cursor(&self) -> Result<Cursor> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        // Removed controls must not linger in the hit-test order.
        state.rebuild_order();
        Ok(state.cursor())
    }
}

impl Node {
    /// Sets the cursor shown over this control and, unless they set their own,
    /// its descendants; `None` restores the default. Text fields keep their
    /// I-beam unless this is set on the field itself.
    pub fn set_cursor(&self, cursor: Option<Cursor>) -> Result {
        self.change(|state, id| {
            match cursor {
                Some(cursor) => state.decorations.entry(id).or_default().cursor = Some(cursor),
                None => {
                    if let Some(decoration) = state.decorations.get_mut(&id) {
                        decoration.cursor = None;
                        state.trim_decoration(id);
                    }
                }
            }
            Ok(())
        })
    }

    /// The cursor set on this control itself, if any.
    pub fn cursor(&self) -> Result<Option<Cursor>> {
        self.change(|state, id| Ok(state.explicit_cursor(id)))
    }
}
