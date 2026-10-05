//! On-demand glyph images, independent of shaping and window systems.
//!
//! A [`GlyphCache`] accepts the same [`FontData`] handles as Parley, but never
//! retains font bytes. Blob identities must name immutable data; do not reuse a
//! manually constructed blob ID for different contents. Images borrow the cache
//! so callers cannot pin evicted images beyond its byte budget.
//!
//! The image limit covers cached pixels and an incoming image. Outline points,
//! decoder workspaces, hash/LRU metadata and Swash scaling/hinting caches are
//! separate costs. [`GlyphCache::release_scratch`] releases scaling workspaces;
//! [`GlyphCache::clear`] releases all retained allocations.
//!
//! [`decode_png`] (and, with the `scene` feature, `decode_image`) turn PNG files
//! into straight sRGB RGBA8 under explicit size limits; the cache uses the same
//! decoder for color-font strikes.
//!
//! Supports grayscale outlines, COLRv0 palette layers, and embedded PNG/BGRA/
//! alpha bitmaps. Color sampling and layer composition use linear-light
//! premultiplied arithmetic, then expose straight sRGB pixels. COLRv1 glyphs
//! return [`GlyphError::UnsupportedGlyph`]; SVG-in-OpenType is not rendered.

mod bitmap;
mod cache;
mod decode;
mod key;
mod raster;
#[cfg(feature = "scene")]
mod transform;

pub use cache::GlyphCache;
#[cfg(feature = "scene")]
pub use decode::decode_image;
pub use decode::{
    DEFAULT_MAX_BYTES, DecodeError, DecodedImage, MAX_EXTENT, decode_png, decode_png_with_limit,
};
pub use key::{GlyphKey, OwnedGlyphKey};
pub use linebender_resource_handle::{Blob, FontData};
pub use swash::zeno::Placement;
#[cfg(feature = "scene")]
pub use transform::{GlyphOrigin, RasterTransform};

/// Coverage contrast for a mask glyph painted in a straight sRGB `color`.
///
/// Linear-light blending thins dark text on light backgrounds and thickens light
/// text on dark ones. Renderers map mask coverage `c` to `c + c * (1 - c) * k`,
/// where `k` is this value in `[-1, 1]`: dark text gains weight, light text loses
/// some, and empty or full coverage is unchanged. The curve stays monotonic.
pub fn mask_contrast(color: [u8; 4]) -> f32 {
    let [r, g, b, _] = color.map(|channel| f32::from(channel) / 255.0);
    // Encoded luma approximates perceived lightness without a transfer function.
    1.0 - 2.0 * (0.2126 * r + 0.7152 * g + 0.0722 * b)
}

/// Pixel data interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    /// One coverage byte per pixel; the renderer supplies the text color.
    Mask,
    /// Straight (unpremultiplied) sRGB RGBA, four bytes per pixel.
    Color,
}

/// Image borrowed until the next mutable cache operation.
#[derive(Clone, Copy, Debug)]
pub struct Glyph<'a> {
    /// Baseline-relative placement: draw at `(baseline_x + left, baseline_y - top)`.
    pub placement: Placement,
    /// Tightly packed, top-to-bottom pixel format.
    pub content: Content,
    /// Pixel bytes; empty for glyphs such as spaces.
    pub data: &'a [u8],
}

/// Raster settings that participate in glyph identity.
#[derive(Clone, Copy, Debug)]
pub struct RasterOptions<'a> {
    /// Finite, positive pixels per em, no greater than 16,384.
    pub size: f32,
    /// Fractional baseline offset in device pixels, each in `[0, 1)`.
    /// Positive y moves down, matching screen coordinates.
    pub offset: [f32; 2],
    /// Normalized 2.14 font variation coordinates in axis order (at most 64).
    pub normalized_coords: &'a [i16],
    /// Grid-fit glyph outlines while preserving layout advances.
    pub hint: bool,
    /// Straight sRGB RGBA used by foreground layers of a COLR glyph.
    /// Ignored in grayscale mask cache identity, so theme colors share pixels.
    pub foreground: [u8; 4],
}

impl Default for RasterOptions<'_> {
    fn default() -> Self {
        Self {
            size: 16.0,
            offset: [0.0; 2],
            normalized_coords: &[],
            hint: true,
            foreground: [0, 0, 0, 255],
        }
    }
}

/// Independent limits; a zero entry limit disables storing new glyphs.
#[derive(Clone, Copy, Debug)]
pub struct CacheLimits {
    /// Maximum bytes of retained and incoming glyph pixels together.
    /// A zero limit only permits empty glyphs.
    pub image_bytes: usize,
    /// Maximum number of glyph entries, including empty glyphs.
    pub entries: u32,
    /// Maximum decoded source bitmap bytes, separately from scaled images.
    /// Also the PNG decoder's internal allocation allowance.
    pub bitmap_bytes: usize,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            image_bytes: 2 * 1024 * 1024,
            entries: 4096,
            bitmap_bytes: 2 * 1024 * 1024,
        }
    }
}

/// Observable image and table occupancy; not total process or module memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Bytes in cached image buffers.
    pub image_bytes: usize,
    /// Live glyph entries.
    pub entries: u32,
    /// Reserved LRU slots, which include keys and image metadata.
    pub entry_capacity: u32,
    /// Reserved hash entries, each containing a slot index.
    pub index_capacity: usize,
}

/// Invalid input, unsupported glyph representation, or a resource limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphError {
    /// The font, collection index, glyph ID, or embedded data is invalid.
    InvalidFont,
    /// Raster size, offsets, or variation coordinates are invalid.
    InvalidOptions,
    /// A glyph origin or image transform exceeds the finite device-coordinate range.
    Coordinates,
    /// Glyph pixels exceed the image limit, or no cache entry is permitted.
    ImageBudget,
    /// Source bitmap or its decoder exceeds the separate bitmap allowance.
    BitmapBudget,
    /// This glyph has no supported outline or bitmap representation.
    UnsupportedGlyph,
}

impl core::fmt::Display for GlyphError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidFont => "invalid font or glyph data",
            Self::InvalidOptions => "invalid glyph raster options",
            Self::Coordinates => "glyph transform exceeds the device-coordinate range",
            Self::ImageBudget => "glyph image cache budget exceeded",
            Self::BitmapBudget => "source bitmap budget exceeded",
            Self::UnsupportedGlyph => "unsupported glyph representation",
        })
    }
}

impl std::error::Error for GlyphError {}

struct Image {
    placement: Placement,
    content: Content,
    data: Box<[u8]>,
}

impl Image {
    fn as_glyph(&self) -> Glyph<'_> {
        Glyph {
            placement: self.placement,
            content: self.content,
            data: &self.data,
        }
    }
}
