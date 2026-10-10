use aegle_layout::{Overflow, Style};
use aegle_theme::{ControlKind, Theme};
use aegle_ui::{
    Container, Control, Result, bar, container_style,
    control::{Frame, PaintCx},
    scroll_padding,
};

/// The control of a [`Widgets::scroll_view`](crate::Widgets::scroll_view)
/// container: a viewport whose border and scrollbars draw over its children.
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
        bar::paint(cx.builder, cx.bars, cx.bar_color)?;
        Ok(())
    }
}

pub(crate) fn create(container: &Container) -> Container {
    Container(crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        style.padding = scroll_padding(theme);
        style.overflow.x = Overflow::Scroll;
        style.overflow.y = Overflow::Scroll;
        style.flex_shrink = 0.0;
        Ok((Box::new(ScrollControl) as Box<dyn Control>, style))
    }))
}
