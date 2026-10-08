//! Images.

use aegle_core::Dirty;
use aegle_layout::{AlignItems, Style};
use aegle_scene::{Image, Rect};
use aegle_theme::ControlKind;
use aegle_types::Size;
use aegle_ui::{
    Container, Control, Result,
    control::{MeasureCx, PaintCx},
    handle,
};

handle! {
    /// A retained image stretched over its bounds; it is not focusable.
    pub ImageView(ImageControl)
}

impl ImageView {
    /// Returns the shared image.
    pub fn image(&self) -> Result<Image> {
        self.read(|image| image.0.clone())
    }
    /// Replaces the image and its intrinsic size.
    pub fn set_image(&self, image: &Image) -> Result {
        self.change(|state, id| {
            state
                .tree
                .update(id, Dirty::LAYOUT | Dirty::PAINT, |node| {
                    node.context.control = Box::new(ImageControl(image.clone()));
                })?;
            Ok(())
        })
    }
}

/// The control inside an [`ImageView`] node.
pub struct ImageControl(Image);

impl Control for ImageControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn measure(&mut self, _: &MeasureCx<'_>) -> Result<Size> {
        Ok(Size::new(self.0.width() as f32, self.0.height() as f32))
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        cx.builder
            .image(&self.0, Rect::new(0.0, 0.0, cx.size.width, cx.size.height))?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.node.set_role(aegle_ui::accesskit::Role::Image);
    }
}

pub(crate) fn image(container: &Container, image: &Image) -> Result<ImageView> {
    crate::add(container, |_, _| {
        Ok((
            Box::new(ImageControl(image.clone())) as Box<dyn Control>,
            Style {
                align_self: Some(AlignItems::START),
                flex_shrink: 0.0,
                ..Default::default()
            },
        ))
    })
    .map(ImageView)
}
