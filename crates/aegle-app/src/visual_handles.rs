use std::ops::Deref;

use aegle_core::Dirty;
use aegle_layout::{AlignItems, Style};
use aegle_scene::{Image, SceneBuilder};
use aegle_types::Size;

use crate::{Container, Node, Result, handles::handle, state::Content};

/// Records custom scene commands in local coordinates for a canvas of `Size`.
pub(crate) type Painter = dyn FnMut(&mut SceneBuilder, Size) -> Result;

handle!(
    ImageView,
    "A retained image stretched over its bounds; it is not focusable."
);
handle!(
    Canvas,
    "A retained custom drawing whose painter re-records only after invalidation or resize."
);

impl Container {
    /// Appends an image whose intrinsic logical size is its pixel size. It keeps
    /// that size on the cross axis instead of stretching; `set_size` overrides it.
    pub fn image(&self, image: &Image) -> Result<ImageView> {
        self.add(|_| {
            Ok((
                Content::Image(image.clone()),
                Style {
                    align_self: Some(AlignItems::START),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            ))
        })
        .map(ImageView)
    }

    /// Appends a canvas drawn by `painter` with zero intrinsic size; give it a
    /// size or flex grow. The painter runs during refresh while the UI is
    /// borrowed, so it must not use UI handles. Drawing is not clipped to the bounds.
    pub fn canvas(
        &self,
        painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
    ) -> Result<Canvas> {
        self.add(|_| {
            Ok((
                Content::Canvas(Box::new(painter)),
                Style {
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            ))
        })
        .map(Canvas)
    }
}

impl ImageView {
    /// Returns the shared image.
    pub fn image(&self) -> Result<Image> {
        self.change(
            |state, id| match &state.tree.get(id).unwrap().context.content {
                Content::Image(image) => Ok(image.clone()),
                _ => unreachable!(),
            },
        )
    }
    /// Replaces the image and its intrinsic size.
    pub fn set_image(&self, image: &Image) -> Result {
        self.change(|state, id| {
            state
                .tree
                .update(id, Dirty::LAYOUT | Dirty::PAINT, |node| {
                    node.context.content = Content::Image(image.clone());
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
                node.context.content = Content::Canvas(Box::new(painter));
            })?;
            Ok(())
        })
    }
}
