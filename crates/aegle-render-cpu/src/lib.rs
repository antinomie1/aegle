//! Shared CPU compositing for Aegle scenes, over a pluggable coverage rasterizer.
//!
//! Framebuffers are borrowed and tightly packed. This crate owns everything that
//! is the same for every CPU renderer: the linear-light source-over compositor,
//! nested rectangular and rounded clips with a mask budget, offscreen layers and
//! the three-box backdrop blur, analytic gradients and shadows, bilinear image
//! sampling, and glyph sampling from [`aegle_glyph`]. Only antialiased geometry
//! coverage is delegated: a [`Rasterizer`] turns a shape or path into 8-bit
//! coverage, which the compositor blends.
//!
//! Keeping coverage separate from blending is what makes the CPU backends agree.
//! Colors are decoded once and blended in linear light, then quantized to
//! premultiplied sRGB RGBA8; a rasterizer never touches color. Pixel-aligned
//! geometry is not sent to a rasterizer at all — upright sharp-cornered fills and
//! borders use exact per-pixel area coverage here, so every backend produces the
//! same bytes for them.
//!
//! One coverage mask and one mask per active clip are reused across frames; their
//! total size is limited by the caller. Transient path and scanline allocations
//! inside a rasterizer are separate from the mask budget.

mod blend;
mod bounds;
mod coverage;
mod effects;
mod layer;
pub mod outline;
mod raster;
mod shape;
mod surface;
#[cfg(feature = "text")]
mod text;
mod vector;

pub use aegle_scene::FillRule;
pub use coverage::{Area, Mask};
pub use outline::Segment;
pub use raster::{Frame, Renderer};
pub use surface::Surface;

/// A rendering failure. Do not present a frame after any drawing call fails.
/// Earlier operations in that frame may already have changed its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// Invalid framebuffer dimensions or a byte slice of the wrong length.
    SurfaceSize,
    /// Required coverage/clip masks exceed the configured byte limit.
    MaskBudget {
        /// Bytes needed for this draw's masks.
        required: usize,
        /// Configured mask limit in bytes.
        limit: usize,
    },
    /// Memory for a mask could not be reserved.
    Allocation,
    /// Scene and external clips nest more than eight deep.
    ClipDepth,
    /// Transformed geometry exceeds the rasterizer's representable range.
    Coordinates,
    /// The scene requests a capability not enabled in this renderer build.
    UnsupportedCommand,
    /// Open layers exceed the effect budget.
    EffectBudget {
        /// Bytes the open layers would hold.
        required: usize,
        /// Configured effect limit in bytes.
        limit: usize,
    },
    /// A layer was popped without a matching push.
    UnbalancedLayer,
    /// The system library a backend rasterizes through is not installed.
    BackendUnavailable {
        /// Shared library the backend loads at run time.
        library: &'static str,
    },
    /// The coverage rasterizer failed; `status` is backend-defined.
    Backend {
        /// Backend status code, such as a cairo status.
        status: i32,
    },
    /// Glyph generation or its resource budget failed.
    #[cfg(feature = "text")]
    Glyph(aegle_glyph::GlyphError),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceSize => f.write_str("invalid RGBA framebuffer size"),
            Self::MaskBudget { required, limit } => {
                write!(f, "masks require {required} bytes; limit is {limit}")
            }
            Self::Allocation => f.write_str("could not allocate rendering mask"),
            Self::ClipDepth => f.write_str("clip depth exceeds eight layers"),
            Self::Coordinates => f.write_str("geometry exceeds software rasterizer range"),
            Self::UnsupportedCommand => {
                f.write_str("scene capability is not enabled in this renderer")
            }
            Self::EffectBudget { required, limit } => {
                write!(
                    f,
                    "layers require {required} bytes; effect limit is {limit}"
                )
            }
            Self::UnbalancedLayer => f.write_str("layer pop has no matching push"),
            Self::BackendUnavailable { library } => {
                write!(f, "system library {library} is not available")
            }
            Self::Backend { status } => {
                write!(f, "coverage rasterizer failed with status {status}")
            }
            #[cfg(feature = "text")]
            Self::Glyph(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RenderError {}

/// Antialiased coverage rasterization for the CPU compositor.
///
/// An implementation owns its path representation, its coverage buffer type and
/// any rasterizer context, and writes only coverage: the compositor supplies the
/// color. Every backend must rasterize the same geometry through
/// [`outline::shape_segments`] and [`outline::path_segments`], so outlines agree
/// exactly and only curve flattening and edge sampling can differ.
///
/// Implementations must agree with the compositor's pixel-alignment rules — a
/// device edge on a whole pixel covers exactly that pixel boundary, and one at a
/// half pixel covers half of each neighbour — because the compositor's analytic
/// fast path already relies on them for upright shapes.
pub trait Rasterizer {
    /// A device-space outline built from a scene shape or path.
    type Path;
    /// 8-bit coverage storage this backend can write directly.
    type Mask: Mask;

    /// Builds the device-space outline of a scene shape, optionally as the ring
    /// of a centered border of `stroke` local width.
    ///
    /// Returns [`RenderError::Coordinates`] when the result leaves the
    /// rasterizer's representable range.
    fn shape(
        &mut self,
        shape: aegle_scene::RoundedRect,
        stroke: Option<f32>,
        transform: aegle_scene::Affine,
    ) -> Result<Self::Path, RenderError>;

    /// Builds the device-space outline of a scene path, filled or stroked.
    fn path(
        &mut self,
        path: &aegle_scene::Path,
        stroke: Option<aegle_scene::Stroke>,
        transform: aegle_scene::Affine,
    ) -> Result<Self::Path, RenderError>;

    /// Conservative device-space bounds of the drawn result, including the
    /// spread of a stroke.
    fn bounds(&self, path: &Self::Path) -> aegle_scene::Rect;

    /// Fills `path` into `area` of `coverage`.
    ///
    /// The compositor already cleared those pixels. Only `area` is read back
    /// afterwards, so other pixels may keep the result of an earlier draw.
    fn fill(
        &mut self,
        path: &Self::Path,
        rule: FillRule,
        area: Area,
        coverage: &mut Self::Mask,
    ) -> Result<(), RenderError>;

    /// Recycles an outline no longer needed by the compositor.
    fn release(&mut self, path: Self::Path);
}
