//! Scrollbar gestures: hit testing, thumb dragging and colors. Geometry and
//! painting are in [`crate::bar`].
use crate::bar::Bar;
use aegle_controls::{PointerId, PointerKind};
use aegle_core::{Dirty, NodeId};
use aegle_scene::Color;
use aegle_theme::Appearance;
use aegle_types::Point;

use crate::{Result, state::State};

#[derive(Clone, Copy)]
pub struct Drag {
    pub pointer: PointerId,
    pub node: NodeId,
    pub vertical: bool,
    pub grab: f32,
}

impl State {
    /// Vertical then horizontal bar for a scroll view, or the vertical bar of a
    /// multiline editor. Uses the last refreshed bounds, offset and limits.
    pub fn scrollbars(&self, id: NodeId) -> [Option<Bar>; 2] {
        let element = &self.tree.get(id).unwrap().context;
        let horizontal_allowed = if element.control.viewport() {
            true
        } else if element
            .control
            .editor()
            .is_some_and(|f| f.editor().is_multiline())
        {
            false
        } else {
            return [None, None];
        };
        Bar::layout(
            element.bounds.size,
            element.scroll,
            self.scroll_limit(id),
            horizontal_allowed,
            element.rtl,
        )
    }

    /// Topmost visible bar strip under a window point. Outer scroll views draw
    /// their bars after nested content, so they also win overlapping hits.
    pub fn scrollbar_at(&self, position: Point, editor: Option<NodeId>) -> Option<(NodeId, Bar)> {
        let views = self.overlays.iter().rev().map(|&(_, id)| id);
        views.chain(editor).find_map(|id| {
            let element = &self.tree.get(id).unwrap().context;
            if !element.effective_visible
                || !self.usable(id)
                || !element.clip.is_none_or(|clip| clip.contains(position))
            {
                return None;
            }
            let local = Point::new(
                position.x - element.bounds.origin.x,
                position.y - element.bounds.origin.y,
            );
            self.scrollbars(id)
                .into_iter()
                .flatten()
                .find(|bar| bar.strip.contains(local))
                .map(|bar| (id, bar))
        })
    }

    /// Consumes pointer input belonging to a scrollbar gesture. Other controls
    /// receive no events while a bar is dragged.
    pub fn scrollbar_pointer(
        &mut self,
        pointer: PointerId,
        kind: PointerKind,
        position: Point,
        hit: Option<NodeId>,
    ) -> Result<bool> {
        if let Some(drag) = self.drag.filter(|drag| drag.pointer == pointer) {
            match kind {
                PointerKind::Move => self.drag_to(drag, position)?,
                PointerKind::Down { .. }
                | PointerKind::ButtonDown(_)
                | PointerKind::ButtonUp(_) => {}
                _ => self.end_drag()?,
            }
            return Ok(true);
        }
        let PointerKind::Down { .. } = kind else {
            return Ok(false);
        };
        let editor = hit.filter(|&id| {
            self.tree
                .get(id)
                .unwrap()
                .context
                .control
                .editor()
                .is_some()
        });
        let Some((node, bar)) = self.scrollbar_at(position, editor) else {
            return Ok(false);
        };
        let origin = self.tree.get(node).unwrap().context.bounds.origin;
        let along = bar.along(Point::new(position.x - origin.x, position.y - origin.y));
        let grab = bar.grab(along);
        let drag = Drag {
            pointer,
            node,
            vertical: bar.vertical,
            grab,
        };
        self.drag = Some(drag);
        self.tree.mark_dirty(node, Dirty::PAINT)?;
        self.repaint = true;
        self.drag_to(drag, position)?;
        Ok(true)
    }

    fn drag_to(&mut self, drag: Drag, position: Point) -> Result {
        let Some(bar) = self.scrollbars(drag.node)[usize::from(!drag.vertical)] else {
            return Ok(());
        };
        let origin = self.tree.get(drag.node).unwrap().context.bounds.origin;
        let along = bar.along(Point::new(position.x - origin.x, position.y - origin.y));
        let Some(fraction) = bar.fraction(along, drag.grab) else {
            return Ok(());
        };
        let limit = self.scroll_limit(drag.node);
        let mut offset = self.tree.get(drag.node).unwrap().context.scroll;
        if drag.vertical {
            offset.y = fraction * limit.y;
        } else {
            offset.x = fraction * limit.x;
        }
        self.scroll_to(drag.node, offset)?;
        self.update_geometry()
    }

    /// Ends a bar drag without changing its final offset.
    pub fn end_drag(&mut self) -> Result {
        if let Some(drag) = self.drag.take() {
            if self.tree.get(drag.node).is_some() {
                self.tree.mark_dirty(drag.node, Dirty::PAINT)?;
            }
            self.repaint = true;
            self.rehit_pointer()?;
        }
        Ok(())
    }

    /// Track and thumb colors from the node's [`Appearance::scrollbar`]: the
    /// active thumb while its viewport is hovered or the thumb dragged.
    pub fn scrollbar_color(&self, id: NodeId, appearance: &Appearance) -> [Color; 2] {
        let active = self.drag.is_some_and(|drag| drag.node == id)
            || (self.hover == Some(id) && self.tree.get(id).unwrap().context.control.viewport());
        let [track, rest, hot] = appearance.scrollbar;
        [track, if active { hot } else { rest }]
    }
}
