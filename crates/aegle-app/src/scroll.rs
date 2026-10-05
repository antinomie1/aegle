use aegle_core::{Dirty, NodeId};
use aegle_types::Point;
use aegle_widgets::{clamp_anchor, intersection, reveal_delta, scrollbar::FOOTPRINT};

use crate::{
    Result,
    state::{Content, State},
};

impl State {
    pub fn scroll_limit(&self, id: NodeId) -> Point {
        let node = self.tree.get(id).unwrap();
        match &node.context.content {
            Content::Scroll(_) => {
                Point::new(node.layout().scroll_width(), node.layout().scroll_height())
            }
            Content::Field(field) => {
                let padding = node.context.inset(&self.theme) * 2.0;
                let viewport = node.context.bounds.size;
                let text = field.editor().size();
                Point::new(
                    (text.width + field.editor().ime_rect().size.width
                        - (viewport.width - padding).max(0.0))
                    .max(0.0),
                    (text.height - (viewport.height - padding).max(0.0)).max(0.0),
                )
            }
            _ => Point::default(),
        }
    }

    pub fn scroll_to(&mut self, id: NodeId, offset: Point) -> Result<bool> {
        let limit = self.scroll_limit(id);
        let element = &mut self.tree.get_mut(id).unwrap().context;
        if !element.effective_visible {
            return Ok(false);
        }
        let offset = Point::new(offset.x.clamp(0.0, limit.x), offset.y.clamp(0.0, limit.y));
        if element.scroll == offset {
            return Ok(false);
        }
        element.scroll = offset;
        // Scrolling a view moves retained child records; only its bars repaint.
        if matches!(element.content, Content::Scroll(_)) {
            self.geometry_dirty = true;
        }
        self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        self.repaint = true;
        self.ime_dirty = true;
        Ok(true)
    }

    /// Geometry changes translate retained scenes without reshaping their text.
    pub fn update_geometry(&mut self) -> Result {
        if !std::mem::take(&mut self.geometry_dirty) {
            return Ok(());
        }
        for index in 0..self.order.len() {
            let id = self.order[index];
            let parent = self.tree.parent(id)?.map(|pid| {
                let parent = &self.tree.get(pid).unwrap().context;
                let scrolling = matches!(parent.content, Content::Scroll(_));
                let clip = if scrolling {
                    // Content stops where an overflowing axis's bar begins, so it
                    // never scrolls underneath it.
                    let mut view = parent.bounds;
                    let limit = self.scroll_limit(pid);
                    if limit.y > 0.0 {
                        view.size.width = (view.size.width - FOOTPRINT).max(0.0);
                    }
                    if limit.x > 0.0 {
                        view.size.height = (view.size.height - FOOTPRINT).max(0.0);
                    }
                    Some(parent.clip.map_or(view, |clip| intersection(clip, view)))
                } else {
                    parent.clip
                };
                (
                    parent.bounds.origin,
                    parent.effective_visible,
                    clip,
                    if scrolling {
                        parent.scroll
                    } else {
                        Point::default()
                    },
                )
            });
            let limit = self.scroll_limit(id);
            let node = self.tree.get_mut(id).unwrap();
            let mut bounds = node.bounds();
            bounds.origin.x += node.context.offset.x;
            bounds.origin.y += node.context.offset.y;
            let mut visible = node.context.visible;
            let mut clip = None;
            if let Some((origin, parent_visible, parent_clip, offset)) = parent {
                bounds.origin.x += origin.x - offset.x;
                bounds.origin.y += origin.y - offset.y;
                visible &= parent_visible;
                clip = parent_clip;
            }
            let element = &mut node.context;
            let old_offset = element.scroll;
            if visible && matches!(element.content, Content::Scroll(_)) {
                element.scroll.x = element.scroll.x.min(limit.x);
                element.scroll.y = element.scroll.y.min(limit.y);
            }
            let changed = element.bounds != bounds
                || element.clip != clip
                || element.effective_visible != visible
                || old_offset != element.scroll;
            element.bounds = bounds;
            element.clip = clip;
            element.effective_visible = visible;
            if changed {
                let bars = old_offset != element.scroll;
                self.tree.mark_dirty(
                    id,
                    if bars {
                        Dirty::PAINT | Dirty::SEMANTICS
                    } else {
                        Dirty::SEMANTICS
                    },
                )?;
            }
        }
        self.rehit_pointer()?;
        Ok(())
    }

    pub fn scroll_by_at(&mut self, position: Point, mut delta: Point) -> Result<bool> {
        let repaint = self.refresh()?;
        self.repaint |= repaint;
        let mut target = self.order.iter().rev().copied().find(|&id| {
            let element = &self.tree.get(id).unwrap().context;
            element.effective_visible
                && self.usable(id)
                && element.bounds.contains(position)
                && element.clip.is_none_or(|clip| clip.contains(position))
        });
        let mut changed = false;
        while let Some(id) = target {
            let element = &self.tree.get(id).unwrap().context;
            if matches!(element.content, Content::Scroll(_) | Content::Field(_)) {
                let old = element.scroll;
                changed |= self.scroll_to(id, Point::new(old.x + delta.x, old.y + delta.y))?;
                let new = self.tree.get(id).unwrap().context.scroll;
                delta.x -= new.x - old.x;
                delta.y -= new.y - old.y;
                if delta.x == 0.0 && delta.y == 0.0 {
                    break;
                }
            }
            target = self.tree.parent(id)?;
        }
        self.update_geometry()?;
        Ok(changed)
    }

    pub fn reveal(&mut self, id: NodeId) -> Result {
        self.update_geometry()?;
        let element = &self.tree.get(id).unwrap().context;
        if !element.effective_visible {
            return Ok(());
        }
        let mut rect = element.bounds;
        let mut caret = if let Content::Field(field) = &element.content {
            let padding = element.inset(&self.theme);
            let mut caret = field.editor().ime_rect();
            caret.origin.x += element.bounds.origin.x + padding - element.scroll.x;
            caret.origin.y += element.bounds.origin.y + padding - element.scroll.y;
            Some(clamp_anchor(caret, element.bounds))
        } else {
            None
        };
        let mut parent = self.tree.parent(id)?;
        while let Some(id) = parent {
            let element = &self.tree.get(id).unwrap().context;
            if matches!(element.content, Content::Scroll(_)) {
                let viewport = element.bounds;
                // Reveal a complete control when it fits. Oversized editors use
                // their caret on the constrained axis instead of hiding it again.
                if let Some(caret) = caret {
                    if rect.size.width > viewport.size.width {
                        rect.origin.x = caret.origin.x;
                        rect.size.width = caret.size.width;
                    }
                    if rect.size.height > viewport.size.height {
                        rect.origin.y = caret.origin.y;
                        rect.size.height = caret.size.height;
                    }
                }
                let old = element.scroll;
                self.scroll_to(
                    id,
                    Point::new(
                        old.x
                            + reveal_delta(
                                rect.origin.x,
                                rect.size.width,
                                viewport.origin.x,
                                viewport.size.width,
                            ),
                        old.y
                            + reveal_delta(
                                rect.origin.y,
                                rect.size.height,
                                viewport.origin.y,
                                viewport.size.height,
                            ),
                    ),
                )?;
                let new = self.tree.get(id).unwrap().context.scroll;
                rect.origin.x -= new.x - old.x;
                rect.origin.y -= new.y - old.y;
                rect = clamp_anchor(rect, viewport);
                if let Some(caret) = &mut caret {
                    caret.origin.x -= new.x - old.x;
                    caret.origin.y -= new.y - old.y;
                    *caret = clamp_anchor(*caret, viewport);
                }
            }
            parent = self.tree.parent(id)?;
        }
        self.update_geometry()
    }
}
