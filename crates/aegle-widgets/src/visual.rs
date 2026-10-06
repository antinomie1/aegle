//! Images and custom drawing.

use std::any::Any;

use aegle_core::Dirty;
use aegle_layout::{AlignItems, Style};
use aegle_scene::{Image, Rect, SceneBuilder};
use aegle_theme::ControlKind;
use aegle_types::Size;
use aegle_ui::{
    Container, Control, Result,
    control::{MeasureCx, PaintCx},
    handle,
};

/// Records custom scene commands in local coordinates for a canvas of `Size`.
pub type Painter = dyn FnMut(&mut SceneBuilder, Size) -> Result;

handle!(
    ImageView,
    "A retained image stretched over its bounds; it is not focusable."
);
handle!(
    Canvas,
    "A retained custom drawing whose painter re-records only after invalidation or resize."
);

impl ImageView {
    /// Returns the shared image.
    pub fn image(&self) -> Result<Image> {
        self.change(|state, id| {
            Ok(state
                .control_as::<ImageControl>(id)
                .expect("an image node")
                .0
                .clone())
        })
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

impl Canvas {
    /// Re-records the painter on the next refresh, for example after its data changed.
    pub fn invalidate(&self) -> Result {
        self.change(|state, id| {
            state.tree.mark_dirty(id, Dirty::PAINT)?;
            Ok(())
        })
    }
    /// Replaces the painter and re-records it on the next refresh.
    pub fn set_painter(
        &self,
        painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
    ) -> Result {
        self.change(|state, id| {
            state.tree.update(id, Dirty::PAINT, |node| {
                node.context.control = Box::new(CanvasControl(Box::new(painter)));
            })?;
            Ok(())
        })
    }
}

/// The control inside an [`ImageView`] node.
pub struct ImageControl(Image);

/// The control inside a [`Canvas`] node.
pub struct CanvasControl(Box<Painter>);

impl Control for ImageControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Container
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

impl Control for CanvasControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Container
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        (self.0)(cx.builder, cx.size)
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.node.set_role(aegle_ui::accesskit::Role::Canvas);
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

pub(crate) fn canvas(
    container: &Container,
    painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
) -> Result<Canvas> {
    crate::add(container, |_, _| {
        Ok((
            Box::new(CanvasControl(Box::new(painter))) as Box<dyn Control>,
            Style {
                flex_shrink: 0.0,
                ..Default::default()
            },
        ))
    })
    .map(Canvas)
}
