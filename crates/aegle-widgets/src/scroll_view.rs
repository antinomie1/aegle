use aegle_theme::{ControlKind, Theme};
use aegle_ui::{
    Container, Control, bar, container_style,
    control::{Frame, PaintCx},
    scroll_padding,
};
use aegle_ui::{LayoutStyle, Overflow};

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
    fn retheme(
        &self,
        theme: &Theme,
        local: aegle_ui::LocalLayout,
        root: bool,
        style: &mut LayoutStyle,
    ) {
        aegle_ui::Plain.retheme(theme, local, root, style);
        if !local.contains(aegle_ui::LocalLayout::PADDING) {
            style.set_padding(scroll_padding(theme));
        }
    }
    fn paint_overlay(&mut self, cx: &mut PaintCx<'_>) {
        aegle_ui::paint::outline(
            cx.builder,
            cx.size,
            cx.appearance.radius,
            cx.appearance.border_width,
            cx.appearance.border_color,
        );
        bar::paint(cx.builder, cx.bars, cx.bar_color);
    }
}

pub(crate) fn create(container: &Container) -> Container {
    Container(crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        style
            .set_padding(scroll_padding(theme))
            .set_overflow(Overflow::Scroll, Overflow::Scroll);
        Ok((Box::new(ScrollControl) as Box<dyn Control>, style))
    }))
}
