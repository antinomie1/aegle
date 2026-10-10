// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.

use crate::{
    bar::FOOTPRINT,
    scroll_geometry::{clamp_anchor, intersection, reveal_delta},
};
use aegle_core::{Dirty, NodeId};
use aegle_scene::Affine;
use aegle_types::{Point, Rect};

use crate::{Node, Result, state::State};

impl State {
    /// The largest scroll offset of a viewport, or zero for other nodes.
    pub fn scroll_limit(&self, id: NodeId) -> Point {
        let node = self.tree.get(id).unwrap();
        if node.context.control.viewport() {
            return Point::new(node.layout().scroll_width(), node.layout().scroll_height());
        }
        match node.context.control.editor() {
            Some(field) => {
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
            None => Point::default(),
        }
    }

    /// Scrolls a viewport to `offset`, clamped to its limit; returns whether it moved.
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
        if element.control.viewport() {
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
                let scrolling = parent.control.viewport();
                let clip = if scrolling || parent.clips {
                    // Content stops where an overflowing axis's bar begins, so it
                    // never scrolls underneath it.
                    let mut view = parent.bounds;
                    let limit = if scrolling {
                        self.scroll_limit(pid)
                    } else {
                        Point::default()
                    };
                    if limit.y > 0.0 {
                        let width = (view.size.width - FOOTPRINT).max(0.0);
                        if parent.rtl {
                            view.origin.x += view.size.width - width;
                        }
                        view.size.width = width;
                    }
                    if limit.x > 0.0 {
                        view.size.height = (view.size.height - FOOTPRINT).max(0.0);
                    }
                    let view = parent.xf.map_or(view, |xf| map_rect(xf, view));
                    Some(parent.clip.map_or(view, |clip| intersection(clip, view)))
                } else {
                    parent.clip
                };
                (
                    parent.bounds.origin,
                    parent.effective_visible,
                    clip,
                    if scrolling {
                        self.scroll_shift(pid)
                    } else {
                        Point::default()
                    },
                    parent.xf,
                )
            });
            let limit = self.scroll_limit(id);
            let node = self.tree.get_mut(id).unwrap();
            let mut bounds = node.bounds();
            bounds.origin.x += node.context.offset.x;
            bounds.origin.y += node.context.offset.y;
            let mut visible = node.context.visible;
            let mut clip = None;
            let mut parent_xf = None;
            if let Some((origin, parent_visible, parent_clip, offset, xf)) = parent {
                parent_xf = xf;
                bounds.origin.x += origin.x - offset.x;
                bounds.origin.y += origin.y - offset.y;
                visible &= parent_visible;
                clip = parent_clip;
            }
            let element = &mut node.context;
            let xf = spin_affine(element.spin, bounds, parent_xf);
            let old_offset = element.scroll;
            if visible && element.control.viewport() {
                element.scroll.x = element.scroll.x.min(limit.x);
                element.scroll.y = element.scroll.y.min(limit.y);
            }
            let changed = element.bounds != bounds
                || element.clip != clip
                || element.effective_visible != visible
                || old_offset != element.scroll
                || element.xf != xf;
            element.xf = xf;
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

    /// Scrolls the innermost viewport under `position` that can use `delta`, chaining outward.
    pub fn scroll_by_at(
        &mut self,
        position: Point,
        mut delta: Point,
        wheel: Option<aegle_controls::Modifiers>,
    ) -> Result<bool> {
        let repaint = self.refresh()?;
        self.repaint |= repaint;
        let mut target = self.order.iter().rev().copied().find(|&id| {
            let element = &self.tree.get(id).unwrap().context;
            element.effective_visible
                && self.usable(id)
                && element.bounds.contains(self.untransform(id, position))
                && element.clip.is_none_or(|clip| clip.contains(position))
        });
        let mut changed = false;
        while let Some(id) = target {
            let element = &self.tree.get(id).unwrap().context;
            if let Some(modifiers) = wheel
                && element.control.takes_wheel()
                && self.usable(id)
            {
                let local = self.untransform(id, position);
                let origin = self.tree.get(id).unwrap().context.bounds.origin;
                let input = aegle_controls::Input::Wheel {
                    delta,
                    position: Point::new(local.x - origin.x, local.y - origin.y),
                    modifiers,
                };
                let outcome = self.control(id, input)?;
                self.effects(id, outcome)?;
                if outcome.handled {
                    changed |= outcome.repaint;
                    break;
                }
            }
            let element = &self.tree.get(id).unwrap().context;
            if element.control.viewport() || element.control.editor().is_some() {
                // A right-to-left view's offset grows leftward.
                let sign = if element.rtl && element.control.viewport() {
                    -1.0
                } else {
                    1.0
                };
                let old = element.scroll;
                let target = Point::new(old.x + sign * delta.x, old.y + delta.y);
                changed |= self.scroll_to(id, target)?;
                let new = self.tree.get(id).unwrap().context.scroll;
                delta.x -= sign * (new.x - old.x);
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

    /// Scrolls ancestors so the node is inside their viewports.
    pub fn reveal(&mut self, id: NodeId) -> Result {
        self.update_geometry()?;
        let element = &self.tree.get(id).unwrap().context;
        if !element.effective_visible {
            return Ok(());
        }
        let mut rect = element.bounds;
        let mut caret = if let Some(field) = element.control.editor() {
            let origin = element.text_origin(&self.theme);
            let mut caret = field.editor().ime_rect();
            caret.origin.x += element.bounds.origin.x + origin.x;
            caret.origin.y += element.bounds.origin.y + origin.y;
            Some(clamp_anchor(caret, element.bounds))
        } else {
            None
        };
        let mut parent = self.tree.parent(id)?;
        while let Some(id) = parent {
            let element = &self.tree.get(id).unwrap().context;
            if element.control.viewport() {
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
                let sign = if element.rtl { -1.0 } else { 1.0 };
                let old = element.scroll;
                self.scroll_to(
                    id,
                    Point::new(
                        old.x
                            + sign
                                * reveal_delta(
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
                rect.origin.x -= sign * (new.x - old.x);
                rect.origin.y -= new.y - old.y;
                rect = clamp_anchor(rect, viewport);
                if let Some(caret) = &mut caret {
                    caret.origin.x -= sign * (new.x - old.x);
                    caret.origin.y -= new.y - old.y;
                    *caret = clamp_anchor(*caret, viewport);
                }
            }
            parent = self.tree.parent(id)?;
        }
        self.update_geometry()
    }
}

/// Axis-aligned window-space bounds of a layout-space rectangle.
pub(crate) fn map_rect(xf: Affine, rect: Rect) -> Rect {
    let corners = [
        Point::new(rect.origin.x, rect.origin.y),
        Point::new(rect.origin.x + rect.size.width, rect.origin.y),
        Point::new(rect.origin.x, rect.origin.y + rect.size.height),
        Point::new(
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        ),
    ]
    .map(|p| xf.map_point(p));
    let (mut min, mut max) = (corners[0], corners[0]);
    for p in corners {
        min = Point::new(min.x.min(p.x), min.y.min(p.y));
        max = Point::new(max.x.max(p.x), max.y.max(p.y));
    }
    Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
}

/// Spin about the bounds center, then the parent's presentation.
fn spin_affine(spin: crate::Transform, bounds: Rect, parent: Option<Affine>) -> Option<Affine> {
    if spin == crate::Transform::default() {
        return parent;
    }
    let (sin, cos) = spin.rotation.sin_cos();
    let (a, b) = (spin.scale * cos, spin.scale * sin);
    let (cx, cy) = (
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    // Validated finite and nonzero scale make this invertible.
    let own = Affine::new([a, b, -b, a, cx - a * cx + b * cy, cy - b * cx - a * cy]).ok()?;
    Some(parent.map_or(own, |parent| own.then(parent).unwrap_or(own)))
}

impl Node {
    /// Scrolls ancestor viewports just enough to reveal this control, without changing
    /// focus. Oversized editors reveal their caret on the constrained axis. Layout
    /// is refreshed first. Hidden nodes no-op.
    pub fn ensure_visible(&self) {
        self.change(|state, id| {
            let repaint = state.refresh()?;
            state.repaint |= repaint;
            state.reveal(id)
        })
    }

    /// Last refreshed bounds intersected with ancestor scroll viewports.
    /// Returns `None` when hidden or wholly clipped; does not clip to window edges.
    pub fn visible_bounds(&self) -> Option<Rect> {
        self.change(|state, id| {
            let element = &state.tree.get(id).unwrap().context;
            Ok(if !element.effective_visible || element.bounds.is_empty() {
                None
            } else {
                element.clip.map_or(Some(element.bounds), |clip| {
                    clip.intersection(element.bounds)
                })
            })
        })
    }
}
