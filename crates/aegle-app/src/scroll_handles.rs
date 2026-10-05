use std::ops::Deref;

use aegle_layout::Overflow;
use aegle_types::{Point, Rect, Size};

use crate::{Container, Node, Result, UiError, state::Content, ui::container_style};

/// A retained column with a clipped, independently scrollable viewport.
/// Constrain its size or flex allocation to create overflow. It has no scrollbar
/// widgets; wheel input, focus reveal and explicit offsets share the same state.
#[derive(Clone)]
pub struct ScrollView(pub(crate) Container);

impl Deref for ScrollView {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.0
    }
}

impl Container {
    /// Appends a scrollable column. Children retain their state outside the viewport.
    /// Both axes scroll on overflow; nested views pass unused wheel delta outward.
    pub fn scroll_view(&self) -> Result<ScrollView> {
        self.add(|state| {
            let mut style = container_style(&state.theme, false);
            style.overflow.x = Overflow::Scroll;
            style.overflow.y = Overflow::Scroll;
            style.flex_shrink = 0.0;
            Ok((Content::Scroll(Box::default()), style))
        })
        .map(|node| ScrollView(Container(node)))
    }
}

impl ScrollView {
    /// Current nonnegative logical offset. Hiding preserves this retained value.
    pub fn offset(&self) -> Result<Point> {
        self.change(|state, id| Ok(state.tree.get(id).unwrap().context.scroll))
    }

    /// Maximum offset from the last refreshed layout, including trailing padding.
    /// Hidden layouts have no extent until they are shown and refreshed again.
    pub fn max_offset(&self) -> Result<Point> {
        self.change(|state, id| Ok(state.scroll_limit(id)))
    }

    /// Last refreshed viewport size plus its scrollable overflow on each axis.
    pub fn content_size(&self) -> Result<Size> {
        self.change(|state, id| {
            let viewport = state.tree.get(id).unwrap().context.bounds.size;
            let limit = state.scroll_limit(id);
            Ok(Size::new(
                viewport.width + limit.x,
                viewport.height + limit.y,
            ))
        })
    }

    /// Refreshes layout and scrolls to a finite offset, clamped to current limits.
    /// Negative values select the start. Hidden views retain their previous offset.
    pub fn scroll_to(&self, offset: Point) -> Result {
        finite(offset)?;
        self.change(|state, id| {
            let repaint = state.refresh()?;
            state.repaint |= repaint;
            state.scroll_to(id, offset)?;
            state.update_geometry()
        })
    }

    /// Moves by a finite logical displacement, clamped independently on each axis.
    pub fn scroll_by(&self, delta: Point) -> Result {
        finite(delta)?;
        self.change(|state, id| {
            let repaint = state.refresh()?;
            state.repaint |= repaint;
            let old = state.tree.get(id).unwrap().context.scroll;
            state.scroll_to(id, Point::new(old.x + delta.x, old.y + delta.y))?;
            state.update_geometry()
        })
    }
}

impl Node {
    /// Scrolls ancestor viewports just enough to reveal this control, without changing
    /// focus. Oversized editors reveal their caret on the constrained axis. Layout
    /// is refreshed first. Hidden nodes no-op.
    pub fn ensure_visible(&self) -> Result {
        self.change(|state, id| {
            let repaint = state.refresh()?;
            state.repaint |= repaint;
            state.reveal(id)
        })
    }

    /// Last refreshed bounds intersected with ancestor scroll viewports.
    /// Returns `None` when hidden or wholly clipped; does not clip to window edges.
    pub fn visible_bounds(&self) -> Result<Option<Rect>> {
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

fn finite(point: Point) -> Result {
    if point.x.is_finite() && point.y.is_finite() {
        Ok(())
    } else {
        Err(UiError::InvalidValue.into())
    }
}
