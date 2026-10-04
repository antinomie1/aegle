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
#[cfg(feature = "text")]
mod text;

pub use aegle_types::{Color, Point, Rect};
pub use builder::{Scene, SceneBuilder};
pub use geometry::{Affine, RoundedRect};
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
    /// Draw a positioned glyph run from [`Scene::glyph_runs`].
    #[cfg(feature = "text")]
    Glyphs(usize),
}

/// Invalid geometry or scope usage at the drawing-record boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        })
    }
}

impl core::error::Error for SceneError {}
