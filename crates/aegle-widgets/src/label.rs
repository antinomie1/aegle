//! Display text.

use std::cell::RefCell;

use aegle_layout::Style;
use aegle_scene::Affine;
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::Size;
use aegle_ui::{
    Container, Control, Result,
    control::{MeasureCx, PaintCx},
    handle, text_style,
};

handle! {
    /// A retained display paragraph.
    pub Label(LabelControl): text
}

impl Label {
    /// Replaces text and invalidates its shared layout, scene and semantic state.
    pub fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| state.set_text(id, text))
    }
    /// Copies the current display text.
    pub fn text(&self) -> Result<String> {
        self.change(|state, id| state.text(id))
    }
}

/// The control inside a [`Label`] node.
pub struct LabelControl(Paragraph);

impl LabelControl {
    /// A label showing `text`, styled by `theme`.
    pub fn new(fonts: &RefCell<TextSystem>, text: &str, theme: &Theme) -> Result<Self> {
        Ok(Self(
            fonts.borrow_mut().paragraph(text, &text_style(theme))?,
        ))
    }
}

impl Control for LabelControl {
    fn kind(&self) -> ControlKind {
        ControlKind::Label
    }
    fn default_padding(&self, _: &Theme) -> f32 {
        0.0
    }
    fn paragraph(&self) -> Option<&Paragraph> {
        Some(&self.0)
    }
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        Some(&mut self.0)
    }
    fn baseline(&self, _: Size, padding: f32) -> Option<f32> {
        Some(padding + self.0.first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        let size = self.0.reflow(cx.content_width(), cx.alignment())?;
        Ok(Size::new(
            size.width + 2.0 * cx.padding,
            size.height + 2.0 * cx.padding,
        ))
    }
    fn finalize(&mut self, cx: &MeasureCx<'_>) -> Result {
        self.0.reflow(cx.content_width(), cx.alignment())?;
        Ok(())
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        cx.builder
            .push_transform(Affine::translation(cx.padding, cx.padding)?)?;
        self.0
            .paint_with_color(cx.builder, cx.appearance.foreground)?;
        cx.builder.pop()?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.node.set_role(aegle_ui::accesskit::Role::Label);
        cx.node.set_value(self.0.text());
    }
}

pub(crate) fn create(container: &Container, text: &str) -> Result<Label> {
    crate::add(container, |state, theme| {
        Ok((
            Box::new(LabelControl::new(&state.fonts, text, theme)?),
            Style {
                flex_shrink: 0.0,
                ..Default::default()
            },
        ))
    })
    .map(Label)
}
