//! Small, reusable drawing records independent of windows, layout and renderers.
//!
//! Coordinates use logical pixels. Commands compose in recording order using
//! source-over blending. Colors are unpremultiplied sRGB; renderers perform their
//! documented color conversion before compositing. Transforms and clips apply
//! until the matching [`Command::Pop`]. A clip captures the transform at its push;
//! later transforms do not move it. Strokes are centered on their shape boundary.
//!
//! [`SceneBuilder`] validates geometry and nesting once. Completed scenes expose
//! only an immutable command slice, so backends can rely on those invariants.
//! Backend coordinate or resource limits may still require a rendering error.
#![no_std]

extern crate alloc;

mod builder;
mod geometry;
mod layer;
mod paint;
mod resource;
#[cfg(feature = "text")]
mod text;

pub use aegle_types::{Color, Point, Rect};
pub use builder::{Scene, SceneBuilder};
pub use geometry::{Affine, RoundedRect};
pub use layer::{BlurBox, Layer, blur_boxes, blur_reach};
pub use paint::{Gradient, GradientGeometry, GradientStop};
pub use resource::{FillRule, Image, LineCap, LineJoin, Path, PathBuilder, Stroke, Verb};
#[cfg(feature = "text")]
pub use text::{Blob, FontData, Glyph, GlyphRun};

/// Maximum combined transform and clip nesting depth.
pub const MAX_SCOPE_DEPTH: usize = 64;

/// One validated drawing operation, inspected through [`Scene::commands`].
///
/// Constructing a command directly cannot insert it into a scene. Use the builder
/// to enforce geometry, composed-transform and balanced-scope invariants.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Fill a rectangle with optional uniformly rounded corners.
    Fill {
        /// Shape in the current local coordinate system.
        shape: RoundedRect,
        /// Unpremultiplied sRGB fill color.
        color: Color,
    },
    /// Stroke a shape, centered on its boundary and transformed with the shape.
    Stroke {
        /// Shape in the current local coordinate system.
        shape: RoundedRect,
        /// Unpremultiplied sRGB stroke color.
        color: Color,
        /// Positive local stroke width.
        width: f32,
    },
    /// Push a transform from the new local coordinates into the current ones.
    PushTransform(Affine),
    /// Intersect the active clip with this shape under the current transform.
    PushClip(RoundedRect),
    /// Restore the state preceding the most recent transform or clip push.
    Pop,
    /// Draw [`Scene::images`]`[image]` stretched over `rect`, bilinearly filtered.
    Image {
        /// Index into [`Scene::images`].
        image: usize,
        /// Destination in the current local coordinate system.
        rect: Rect,
    },
    /// Fill [`Scene::paths`]`[path]` using its fill rule.
    FillPath {
        /// Index into [`Scene::paths`].
        path: usize,
        /// Unpremultiplied sRGB fill color.
        color: Color,
    },
    /// Stroke [`Scene::paths`]`[path]`, centered on its outline.
    StrokePath {
        /// Index into [`Scene::paths`].
        path: usize,
        /// Unpremultiplied sRGB stroke color.
        color: Color,
        /// Positive local width, caps and joins.
        stroke: Stroke,
    },
    /// Fill a shape with [`Scene::gradients`]`[gradient]`.
    FillGradient {
        /// Shape in the current local coordinate system, as for the gradient.
        shape: RoundedRect,
        /// Index into [`Scene::gradients`].
        gradient: usize,
    },
    /// Draw the soft shadow of a shape: its coverage convolved with a Gaussian.
    /// Offset and spread are applied by moving or growing `shape`.
    Shadow {
        /// Shape casting the shadow, in the current local coordinate system.
        shape: RoundedRect,
        /// Unpremultiplied sRGB shadow color.
        color: Color,
        /// Positive Gaussian standard deviation in local units; the shadow
        /// fades out within three of them beyond the shape.
        blur: f32,
    },
    /// Draw a positioned glyph run from [`Scene::glyph_runs`].
    #[cfg(feature = "text")]
    Glyphs(usize),
    /// Draw a texture owned by the application and registered with the
    /// renderer, stretched over `rect` and bilinearly filtered. Renderers
    /// without that texture fail the frame instead of skipping it; the
    /// software renderer has no textures and always fails it.
    Texture {
        /// The renderer's identity for the texture.
        texture: TextureId,
        /// Destination in the current local coordinate system.
        rect: Rect,
    },
}

/// A GPU texture owned by the application, identified by the renderer that
/// registered it. Its sampled values are linear premultiplied RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

/// Invalid geometry or scope usage at the drawing-record boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SceneError {
    /// A coordinate, extent, radius or stroke width is not finite.
    NonFinite,
    /// A size, radius or stroke width is negative.
    NegativeExtent,
    /// A transform is singular or has no finite `f32` inverse.
    InvalidTransform,
    /// A composed transform or mapped geometry exceeds the finite `f32` range.
    CoordinateRange,
    /// A push would exceed [`MAX_SCOPE_DEPTH`].
    ScopeLimit,
    /// A pop has no corresponding push.
    UnexpectedPop,
    /// Finishing a scene with one or more open scopes.
    UnclosedScope,
    /// A glyph run has a nonpositive font size or out-of-range variation value.
    InvalidText,
    /// Image extents are zero or too large, or pixels do not match them.
    InvalidImage,
    /// A path segment or close does not follow a `move_to`.
    InvalidPath,
    /// Gradient stops are too few, too many, unordered or outside `0..=1`, or its
    /// line has no length or its radius is not positive.
    InvalidGradient,
}

impl core::fmt::Display for SceneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "drawing geometry must be finite",
            Self::NegativeExtent => "drawing extents must be nonnegative",
            Self::InvalidTransform => "transform must have a finite inverse",
            Self::CoordinateRange => "transformed geometry exceeds the coordinate range",
            Self::ScopeLimit => "drawing scope nesting limit exceeded",
            Self::UnexpectedPop => "drawing scope pop has no matching push",
            Self::UnclosedScope => "drawing scene has unclosed scopes",
            Self::InvalidText => "invalid glyph run size or variation coordinate",
            Self::InvalidImage => "image extents or pixel length are invalid",
            Self::InvalidPath => "path segment does not follow a move",
            Self::InvalidGradient => "gradient stops or geometry are invalid",
        })
    }
}

impl core::error::Error for SceneError {}
