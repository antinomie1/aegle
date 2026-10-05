//! Overlay scrollbars shared by scroll views and multiline editors.
//!
//! Bars take no layout space: a thin square thumb is drawn over the viewport's
//! trailing edge only while that axis overflows. Pressing the thumb drags it;
//! pressing elsewhere on the strip centers the thumb there and keeps dragging.
//! No timer, fade or hover animation is involved.

use aegle_controls::{PointerId, PointerKind};
use aegle_core::{Dirty, NodeId};
use aegle_scene::{Color, Rect, RoundedRect, SceneBuilder};
use aegle_types::Point;

use crate::{
    Result,
    state::{Content, State},
};

/// Pointer strip width along a scrollable edge, in logical pixels.
const STRIP: f32 = 12.0;
/// Visible thumb thickness at the outer edge of the strip.
const THICKNESS: f32 = 6.0;
const MARGIN: f32 = 2.0;
const MIN_THUMB: f32 = 24.0;

/// One overflowing axis in the viewport's local coordinates.
#[derive(Clone, Copy)]
pub(crate) struct Bar {
    pub vertical: bool,
    pub strip: Rect,
    /// Thumb offset and length along the strip.
    pub thumb: f32,
    pub length: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct Drag {
    pub pointer: PointerId,
    pub node: NodeId,
    pub vertical: bool,
    pub grab: f32,
}

impl Bar {
    fn along(&self, point: Point) -> f32 {
        if self.vertical {
            point.y - self.strip.origin.y
        } else {
            point.x - self.strip.origin.x
        }
    }

    fn track(&self) -> f32 {
        if self.vertical {
            self.strip.size.height
        } else {
            self.strip.size.width
        }
    }

    pub fn thumb_rect(&self) -> Rect {
        let s = self.strip;
        if self.vertical {
            let width = THICKNESS.min(s.size.width);
            let x = (s.origin.x + s.size.width - MARGIN - width).max(s.origin.x);
            Rect::new(x, s.origin.y + self.thumb, width, self.length)
        } else {
            let height = THICKNESS.min(s.size.height);
            let y = (s.origin.y + s.size.height - MARGIN - height).max(s.origin.y);
            Rect::new(s.origin.x + self.thumb, y, self.length, height)
        }
    }
}

impl State {
    /// Vertical then horizontal bar for a scroll view, or the vertical bar of a
    /// multiline editor. Uses the last refreshed bounds, offset and limits.
    pub fn scrollbars(&self, id: NodeId) -> [Option<Bar>; 2] {
        let element = &self.tree.get(id).unwrap().context;
        let horizontal_allowed = match &element.content {
            Content::Scroll(_) => true,
            Content::Field(field) if field.editor().is_multiline() => false,
            _ => return [None, None],
        };
        let limit = self.scroll_limit(id);
        let size = element.bounds.size;
        let vertical = limit.y > 0.0;
        let horizontal = horizontal_allowed && limit.x > 0.0;
        let bar = |vertical: bool, track: f32, cross: f32, viewport: f32, offset: f32, max: f32| {
            let track = track.max(0.0);
            let width = STRIP.min(cross.max(0.0));
            let length = (track * viewport / (viewport + max))
                .max(MIN_THUMB)
                .min(track);
            let start = cross - width;
            Bar {
                vertical,
                strip: if vertical {
                    Rect::new(start, 0.0, width, track)
                } else {
                    Rect::new(0.0, start, track, width)
                },
                thumb: (offset / max).clamp(0.0, 1.0) * (track - length),
                length,
            }
        };
        let corner = |other: bool| if other { STRIP } else { 0.0 };
        [
            vertical.then(|| {
                bar(
                    true,
                    size.height - corner(horizontal),
                    size.width,
                    size.height,
                    element.scroll.y,
                    limit.y,
                )
            }),
            horizontal.then(|| {
                bar(
                    false,
                    size.width - corner(vertical),
                    size.height,
                    size.width,
                    element.scroll.x,
                    limit.x,
                )
            }),
        ]
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
                PointerKind::Down { .. } => {}
                _ => self.end_drag()?,
            }
            return Ok(true);
        }
        let PointerKind::Down { .. } = kind else {
            return Ok(false);
        };
        let editor = hit.filter(|&id| {
            matches!(
                &self.tree.get(id).unwrap().context.content,
                Content::Field(_)
            )
        });
        let Some((node, bar)) = self.scrollbar_at(position, editor) else {
            return Ok(false);
        };
        let origin = self.tree.get(node).unwrap().context.bounds.origin;
        let along = bar.along(Point::new(position.x - origin.x, position.y - origin.y));
        let grab = if (bar.thumb..bar.thumb + bar.length).contains(&along) {
            along - bar.thumb
        } else {
            bar.length * 0.5
        };
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
        let travel = bar.track() - bar.length;
        if travel <= 0.0 {
            return Ok(());
        }
        let origin = self.tree.get(drag.node).unwrap().context.bounds.origin;
        let along = bar.along(Point::new(position.x - origin.x, position.y - origin.y));
        let fraction = ((along - drag.grab) / travel).clamp(0.0, 1.0);
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

    /// Thumb color: border at rest, muted while hovered or dragged.
    pub fn scrollbar_color(&self, id: NodeId) -> Color {
        let active = self.drag.is_some_and(|drag| drag.node == id)
            || (self.hover == Some(id)
                && matches!(
                    self.tree.get(id).unwrap().context.content,
                    Content::Scroll(_)
                ));
        if active {
            self.theme.muted
        } else {
            self.theme.border
        }
    }
}

pub(crate) fn paint(
    builder: &mut SceneBuilder,
    bars: [Option<Bar>; 2],
    color: Color,
    radius: f32,
) -> Result {
    for bar in bars.into_iter().flatten() {
        let rect = bar.thumb_rect();
        if !rect.is_empty() {
            builder.fill(RoundedRect::new(rect, radius.min(THICKNESS * 0.5))?, color)?;
        }
    }
    Ok(())
}
