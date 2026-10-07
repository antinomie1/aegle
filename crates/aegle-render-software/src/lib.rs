//! Software rendering with shared scenes, linear-light blending and bounded masks.
//!
//! Framebuffers are borrowed. One coverage mask and one mask per active clip
//! are reused across frames; their total size is limited by the caller. Tiny-skia
//! supplies antialiased geometry and arbitrary path/stroke coverage, with PNG
//! support disabled in normal builds. Images are sampled bilinearly in linear
//! premultiplied space. Tiny-skia's transient path/scanline allocations are
//! separate from the mask budget.

mod blend;
mod bounds;
mod effects;
mod path;
mod raster;
mod shape;
mod surface;
#[cfg(feature = "text")]
mod text;
mod vector;

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
            #[cfg(feature = "text")]
            Self::Glyph(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RenderError {}
