//! Node shadows and gradient backgrounds, drawn with the scene's native
//! effect commands on every backend.

use crate::{Color, Node, Point, Result, UiError, style::Decoration};
use aegle_core::Dirty;
use aegle_scene::{Gradient, GradientGeometry, Rect, RoundedRect, SceneBuilder};
use aegle_types::Size;

/// A soft shadow drawn beneath a node's background, following its corner
/// radius. It extends beyond the node's bounds without affecting layout or
/// hit testing; ancestors' clips still apply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Finite logical displacement of the shadow from the node.
    pub offset: Point,
    /// Nonnegative Gaussian standard deviation in logical pixels; zero is a
    /// hard-edged copy of the shape.
    pub blur: f32,
    /// Finite logical growth (or, when negative, shrinkage) of the shape.
    pub spread: f32,
    /// Unpremultiplied sRGB color.
    pub color: Color,
}

impl Node {
    /// Draws `shadow` beneath this node, or removes it with `None`.
    pub fn set_shadow(&self, shadow: Option<Shadow>) -> Result {
        if let Some(s) = shadow {
            let finite = [s.offset.x, s.offset.y, s.blur, s.spread].map(f32::is_finite);
            if finite.contains(&false) || s.blur < 0.0 {
                return Err(UiError::InvalidValue.into());
            }
        }
        self.change(|state, id| {
            state.decorations.entry(id).or_default().shadow = shadow;
            state.trim_decoration(id);
            state.tree.mark_dirty(id, Dirty::PAINT)?;
            // The old shadow may reach beyond the new one.
            state.repaint = true;
            Ok(())
        })
    }

    /// The shadow set by [`Self::set_shadow`].
    pub fn shadow(&self) -> Result<Option<Shadow>> {
        self.change(|state, id| Ok(state.decorations.get(&id).and_then(|d| d.shadow)))
    }

    /// Fills this node's background with `gradient` instead of its background
    /// color, or restores the color with `None`. The gradient's coordinates are
    /// fractions of the node's size: x of its width and y of its height, with a
    /// radial radius in fractions of its larger side. Controls that paint no
    /// background ignore it.
    pub fn set_background_gradient(&self, gradient: Option<Gradient>) -> Result {
        self.change(|state, id| {
            state.decorations.entry(id).or_default().gradient = gradient;
            state.trim_decoration(id);
            state.tree.mark_dirty(id, Dirty::PAINT)?;
            Ok(())
        })
    }

    /// The gradient set by [`Self::set_background_gradient`].
    pub fn background_gradient(&self) -> Result<Option<Gradient>> {
        self.change(|state, id| Ok(state.decorations.get(&id).and_then(|d| d.gradient.clone())))
    }
}

/// Records a node's shadow, before its background.
pub(crate) fn paint_shadow(
    decoration: Option<&Decoration>,
    builder: &mut SceneBuilder,
    size: Size,
    radius: f32,
) -> Result {
    let Some(shadow) = decoration.and_then(|d| d.shadow) else {
        return Ok(());
    };
    let (width, height) = (
        (size.width + shadow.spread * 2.0).max(0.0),
        (size.height + shadow.spread * 2.0).max(0.0),
    );
    let rect = Rect::new(
        shadow.offset.x - shadow.spread,
        shadow.offset.y - shadow.spread,
        width,
        height,
    );
    let shape = RoundedRect::new(rect, (radius + shadow.spread).max(0.0))?;
    builder.shadow(shape, shadow.color, shadow.blur)?;
    Ok(())
}

/// Records a node's gradient background in place of its color; returns
/// whether it had one.
pub(crate) fn paint_gradient(
    decoration: Option<&Decoration>,
    builder: &mut SceneBuilder,
    shape: RoundedRect,
) -> Result<bool> {
    let Some(gradient) = decoration.and_then(|d| d.gradient.as_ref()) else {
        return Ok(false);
    };
    let size = shape.rect().size;
    let at = |p: Point| Point::new(p.x * size.width, p.y * size.height);
    let geometry = match gradient.geometry() {
        GradientGeometry::Linear { start, end } => GradientGeometry::Linear {
            start: at(start),
            end: at(end),
        },
        GradientGeometry::Radial { center, radius } => GradientGeometry::Radial {
            center: at(center),
            radius: radius * size.width.max(size.height),
        },
    };
    // A collapsed node has no extent for the gradient to span.
    if let Ok(sized) = gradient.with_geometry(geometry) {
        builder.fill_gradient(shape, &sized)?;
    }
    Ok(true)
}
