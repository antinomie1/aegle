//! Right-to-left layout. A node's direction is inherited by its subtree and
//! drives Taffy's mirrored flex, grid and block placement, paragraph alignment,
//! the scrollbar side, horizontal scroll origin and directional controls.

use aegle_core::NodeId;
use aegle_layout::LayoutDirection;
use aegle_types::Point;

use crate::{Node, Result, state::State};

impl Node {
    /// Lays this subtree out right to left or left to right; `None` inherits
    /// the parent's direction, and the root defaults to left to right.
    ///
    /// Right to left, rows start at the right edge, `Start` alignment means the
    /// right side, grid columns run leftward, text aligns right, a viewport's
    /// vertical bar and gutter move to its left edge and horizontal scrolling
    /// starts at the right, and directional controls (sliders, switches, tabs,
    /// steppers, dropdown marks, splitters) mirror their drawing and arrow keys.
    /// Insets, margins and absolute positions stay physical. Paragraphs still
    /// order mixed-direction text by their own content, as Parley detects it.
    pub fn set_layout_direction(&self, direction: impl Into<Option<LayoutDirection>>) {
        let direction = direction.into();
        self.change(|state, id| {
            state.tree.get_mut(id).unwrap().context.direction = direction;
            state.propagate_direction(id)
        })
    }

    /// The resolved direction: this node's, the nearest ancestor's or left to right.
    pub fn layout_direction(&self) -> LayoutDirection {
        self.change(|state, id| Ok(state.tree.get(id).unwrap().style().direction))
    }
}

impl State {
    /// Whether a live node is laid out right to left.
    pub fn rtl(&self, id: NodeId) -> bool {
        self.tree.get(id).unwrap().context.rtl
    }

    /// Re-resolves directions in the subtree of `id` from its parent and the
    /// local settings, relaying out and repainting each node that changed.
    pub fn propagate_direction(&mut self, id: NodeId) -> Result {
        self.rebuild_order();
        let start = self.order.iter().position(|&n| n == id).unwrap();
        for index in start..self.order.len() {
            let n = self.order[index];
            if !self.contains(id, n) {
                break;
            }
            let inherited = self.tree.parent(n)?.map_or(LayoutDirection::Ltr, |p| {
                self.tree.get(p).unwrap().style().direction
            });
            let node = self.tree.get_mut(n).unwrap();
            let direction = node.context.direction.unwrap_or(inherited);
            if node.style().direction == direction {
                continue;
            }
            node.context.rtl = direction == LayoutDirection::Rtl;
            let mut style = node.style().clone();
            style.direction = direction;
            if node.context.control.viewport() {
                // The bar side's padding derives from the other side, which
                // keeps the application's value; see `update_gutters`.
                std::mem::swap(&mut style.padding.left, &mut style.padding.right);
            }
            aegle_layout::set_style(&mut self.tree, n, style)?;
        }
        self.geometry_dirty = true;
        self.ime_dirty = true;
        self.repaint = true;
        Ok(())
    }

    /// How far a viewport moves its children: its scroll offset, which is
    /// measured from the start edge, so it is mirrored horizontally right to left
    /// where content overflows leftward.
    pub fn scroll_shift(&self, id: NodeId) -> Point {
        let element = &self.tree.get(id).unwrap().context;
        if element.rtl {
            Point::new(-element.scroll.x, element.scroll.y)
        } else {
            element.scroll
        }
    }
}
