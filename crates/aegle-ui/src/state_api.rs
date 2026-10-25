//! The engine state as control libraries read and change it; the fields stay
//! private so the engine keeps its invariants.

use std::{cell::RefCell, rc::Rc, time::Instant};

use aegle_core::{Dirty, NodeId};
use aegle_layout::{LayoutDirection, LayoutStyle};
use aegle_text::TextSystem;
use aegle_theme::Theme;
use aegle_types::{Point, Rect, Size};

use crate::{Element, Result, State, control::Control};

impl State {
    /// The root node.
    pub fn root(&self) -> NodeId {
        self.root
    }
    /// The window size in logical pixels.
    pub fn size(&self) -> Size {
        self.size
    }
    /// The shared text system.
    pub fn fonts(&self) -> &Rc<RefCell<TextSystem>> {
        &self.fonts
    }
    /// The UI theme; [`State::theme_of`] resolves a node's local theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }
    /// The focused node.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus.current(&self.tree)
    }
    /// The node under the pointer.
    pub fn hover_target(&self) -> Option<NodeId> {
        self.hover
    }
    /// The last pointer position in window coordinates.
    pub fn pointer_position(&self) -> Option<Point> {
        self.pointer.map(|(_, position)| position)
    }
    /// Asks for a repaint without invalidating any node.
    pub fn request_repaint(&mut self) {
        self.repaint = true;
    }
    /// Asks for layout to run again on the next refresh.
    pub fn invalidate_layout(&mut self) {
        self.geometry_dirty = true;
    }
    /// Calls the installed [`crate::Hooks::wake`] at `at` or earlier.
    pub fn wake_at(&mut self, at: Instant) {
        self.wake = Some(self.wake.map_or(at, |wake| wake.min(at)));
    }
    /// Drops the input method's composition after the focused editor's text
    /// changed outside input-method editing.
    pub fn reset_ime(&mut self) {
        self.ime_dirty = true;
        self.ime_reset = true;
        self.input_method = false;
    }

    /// Whether `id` names a live node.
    pub fn is_alive(&self, id: NodeId) -> bool {
        self.tree.get(id).is_some()
    }
    /// The parent of a live node; `None` for the root.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.tree.parent(id).unwrap()
    }
    /// The children of a live node in order.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.tree.children(id).unwrap()
    }
    /// Child `index` of a live node.
    pub fn child(&self, id: NodeId, index: usize) -> Option<NodeId> {
        self.tree.child(id, index).unwrap()
    }
    /// A live node's engine data.
    pub fn element(&self, id: NodeId) -> &Element {
        &self.tree.get(id).unwrap().context
    }
    /// A live node's engine data, mutably; invalidate what you change.
    pub fn element_mut(&mut self, id: NodeId) -> &mut Element {
        &mut self.tree.get_mut(id).unwrap().context
    }
    /// Marks part of a live node stale.
    pub fn invalidate(&mut self, id: NodeId, dirty: Dirty) -> Result {
        Ok(self.tree.mark_dirty(id, dirty)?)
    }
    /// A live node's layout style.
    pub fn layout_style(&self, id: NodeId) -> &LayoutStyle {
        self.tree.get(id).unwrap().style()
    }
    /// Replaces a live node's layout style and relays it out.
    pub fn set_layout_style(&mut self, id: NodeId, style: LayoutStyle) -> Result {
        Ok(aegle_layout::set_style(&mut self.tree, id, style)?)
    }
    /// A live node's bounds from the last layout, in its parent's space.
    pub fn layout_bounds(&self, id: NodeId) -> Rect {
        self.tree.get(id).unwrap().bounds()
    }
    /// Whether a live node is transparent to layout, see
    /// [`aegle_layout::set_contents`].
    pub fn is_contents(&self, id: NodeId) -> bool {
        self.tree.get(id).unwrap().is_contents()
    }
    /// Moves a live non-root subtree to the end of `parent`'s children.
    pub fn reparent(&mut self, id: NodeId, parent: NodeId) -> Result {
        self.tree.reparent(id, Some(parent))?;
        self.invalidate_structure();
        Ok(())
    }
    /// Sets a live node's own layout direction, `None` to inherit, and
    /// re-resolves its subtree.
    pub fn set_layout_direction(
        &mut self,
        id: NodeId,
        direction: Option<LayoutDirection>,
    ) -> Result {
        self.element_mut(id).direction = direction;
        self.propagate_direction(id)
    }
    /// Drops the action handlers of a live node, see [`State::on_action`].
    pub fn clear_actions(&mut self, id: NodeId) {
        self.callbacks.remove(&id);
    }
}

impl Element {
    /// The control.
    pub fn control(&self) -> &dyn Control {
        &*self.control
    }
    /// The control, mutably; invalidate what you change.
    pub fn control_mut(&mut self) -> &mut dyn Control {
        &mut *self.control
    }
    /// The layout rectangle in window space as of the last refresh.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    /// The clip inherited from scrolling ancestors, in window space.
    pub fn clip(&self) -> Option<Rect> {
        self.clip
    }
    /// The presented translation after layout.
    pub fn offset(&self) -> Point {
        self.offset
    }
    /// Sets the presented translation after layout, inherited by the subtree.
    pub fn set_offset(&mut self, offset: Point) {
        self.offset = offset;
    }
    /// Sets the content padding; `None` uses the control's default.
    pub fn set_padding(&mut self, padding: Option<f32>) {
        self.padding = padding;
    }
    /// The layout properties set on this node.
    pub fn local_layout(&self) -> crate::LocalLayout {
        self.local_layout
    }
    /// The nearest local theme of this node or an ancestor.
    pub fn local_theme(&self) -> Option<&Rc<Theme>> {
        self.theme.as_ref()
    }
    /// Whether the node and every ancestor are shown.
    pub fn is_shown(&self) -> bool {
        self.effective_visible
    }
}
