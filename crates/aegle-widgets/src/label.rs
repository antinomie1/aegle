//! Display text.

use std::cell::RefCell;

use aegle_scene::Affine;
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::Size;
use aegle_ui::LayoutStyle;
use aegle_ui::{
    Container, Control, OrFail, Result,
    control::{MeasureCx, PaintCx},
    handle, text_style,
};

handle! {
    /// A retained display paragraph.
    pub Label(LabelControl): text
}

impl Label {
    /// Replaces text and invalidates its shared layout, scene and semantic state.
    pub fn set_text(&self, text: &str) {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Copies the current display text.
    pub fn text(&self) -> String {
        self.change(|state, id| state.text(id))
    }
    /// Sets whether the label wraps text when its line overflows available width.
    ///
    /// When `wrap` is enabled, text wraps to available layout width and flex shrink
    /// is set to 1.0. When disabled (the default), text stays on a single line
    /// with flex shrink 0.0.
    pub fn set_wrap(&self, wrap: bool) {
        self.update(|control| control.wrap = wrap);
        self.set_shrink(if wrap { 1.0 } else { 0.0 });
    }
    /// Whether text wrapping is enabled.
    pub fn is_wrap(&self) -> bool {
        self.read(|control| control.wrap)
    }
}

/// The control inside a [`Label`] node.
pub struct LabelControl {
    text: Paragraph,
    pub(crate) wrap: bool,
    /// [`crate::kinds::LABEL`], or the kind of a label-like part such as a tooltip.
    pub(crate) kind: &'static ControlKind,
}

impl LabelControl {
    /// A label showing `text`, styled by `theme`.
    pub fn new(fonts: &RefCell<TextSystem>, text: &str, theme: &Theme) -> Result<Self> {
        Ok(Self {
            text: fonts.borrow_mut().paragraph(text, &text_style(theme))?,
            wrap: false,
            kind: &crate::kinds::LABEL,
        })
    }
}

impl Control for LabelControl {
    fn kind(&self) -> &'static ControlKind {
        self.kind
    }
    fn default_padding(&self, _: &Theme) -> f32 {
        0.0
    }
    fn paragraph(&self) -> Option<&Paragraph> {
        Some(&self.text)
    }
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        Some(&mut self.text)
    }
    fn baseline(&self, _: Size, padding: f32) -> Option<f32> {
        Some(padding + self.text.first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Size {
        let width = if self.wrap { cx.content_width() } else { None };
        let size = self.text.reflow(width, cx.alignment()).or_fail();
        Size::new(
            size.width + 2.0 * cx.padding,
            size.height + 2.0 * cx.padding,
        )
    }
    fn finalize(&mut self, cx: &MeasureCx<'_>) {
        let width = if self.wrap { cx.content_width() } else { None };
        self.text.reflow(width, cx.alignment()).or_fail();
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        cx.builder
            .push_transform(Affine::translation(cx.padding, cx.padding));
        self.text
            .paint_with_color(cx.builder, cx.appearance.foreground);
        cx.builder.pop();
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.set_role(aegle_ui::Role::Label);
        cx.set_value(self.text.text());
    }
}

pub(crate) fn create(container: &Container, text: &str) -> Label {
    Label(crate::add(container, |state, theme| {
        Ok(
            (Box::new(LabelControl::new(&state.fonts(), text, theme)?), {
                let mut style = LayoutStyle::default();
                style.set_shrink(0.0);
                style
            }),
        )
    }))
}
