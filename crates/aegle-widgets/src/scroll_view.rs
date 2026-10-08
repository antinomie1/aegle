use std::ops::Deref;

use aegle_layout::{Overflow, Style};
use aegle_theme::{ControlKind, Theme};
use aegle_types::{Point, Size};
use aegle_ui::{
    Container, Control, Result, UiError, bar, container_style,
    control::{Frame, PaintCx},
    scroll_padding,
};

/// A retained column with a clipped, independently scrollable viewport.
/// Constrain its size or flex allocation to create overflow. Overlay scrollbars,
/// wheel input, focus reveal and explicit offsets share the same state.
#[derive(Clone)]
pub struct ScrollView(pub Container);

impl Deref for ScrollView {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.0
    }
}

/// The control inside a [`ScrollView`] node: a viewport whose border and
/// scrollbars draw over its children.
pub struct ScrollControl;

impl Control for ScrollControl {
    fn kind(&self) -> &'static ControlKind {
        &crate::kinds::SCROLL_VIEW
    }
    fn viewport(&self) -> bool {
        true
    }
    fn frame(&self) -> Frame {
        // The border draws after the children, so scrolled content cannot cover it.
        Frame {
            background: true,
            border: false,
        }
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, root: bool, style: &mut Style) {
        aegle_ui::Plain.retheme(theme, local, root, style);
        if !local.contains(aegle_ui::LocalLayout::PADDING) {
            style.padding = scroll_padding(theme);
        }
    }
    fn paint_overlay(&mut self, cx: &mut PaintCx<'_>) -> Result {
        aegle_ui::paint::outline(
            cx.builder,
            cx.size,
            cx.appearance.radius,
            cx.appearance.border_width,
            cx.appearance.border_color,
        )?;
        bar::paint(cx.builder, cx.bars, cx.bar_color, cx.theme.radius)?;
        Ok(())
    }
}

pub(crate) fn create(container: &Container) -> Result<ScrollView> {
    crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        style.padding = scroll_padding(theme);
        style.overflow.x = Overflow::Scroll;
        style.overflow.y = Overflow::Scroll;
        style.flex_shrink = 0.0;
        Ok((Box::new(ScrollControl) as Box<dyn Control>, style))
    })
    .map(|node| ScrollView(Container(node)))
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

fn finite(point: Point) -> Result {
    if point.x.is_finite() && point.y.is_finite() {
        Ok(())
    } else {
        Err(UiError::InvalidValue.into())
    }
}
