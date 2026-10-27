use crate::{Color, Point, SceneError};
use alloc::vec::Vec;

pub use linebender_resource_handle::{Blob, FontData};

/// Font identity of one glyph run, independent of the backend that shaped it.
///
/// Renderers treat it as an opaque identity: they key glyph caches on it and ask
/// the backend named by a [`SystemFont`] to rasterize. A run never contains a
/// native font pointer, so it stays valid across shaping and layout calls.
#[derive(Clone, Debug, PartialEq)]
pub enum FontSource {
    /// Font bytes shared with the application or an embedding host.
    Bytes(FontData),
    /// A font object owned by a platform backend, registered under `backend`.
    System(SystemFont),
}

impl FontSource {
    /// The shared font bytes, or `None` for a platform font handle.
    pub fn bytes(&self) -> Option<&FontData> {
        match self {
            Self::Bytes(font) => Some(font),
            Self::System(_) => None,
        }
    }

    /// The platform font handle, or `None` for shared font bytes.
    pub fn system(&self) -> Option<SystemFont> {
        match self {
            Self::Bytes(_) => None,
            Self::System(font) => Some(*font),
        }
    }
}

impl From<FontData> for FontSource {
    fn from(font: FontData) -> Self {
        Self::Bytes(font)
    }
}

/// Platform font object: `backend` names the crate that can resolve it and
/// `id` is that backend registry's key for one face.
///
/// Both values are opaque to the scene; only the backend that produced the
/// handle interprets `id`, and the handle is meaningful only while that
/// backend's registry entry lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemFont {
    /// Backend discriminator, one of the values this module documents.
    pub backend: u8,
    /// Backend-owned face key.
    pub id: u64,
}

impl SystemFont {
    /// DirectWrite font face handle (`aegle-text/backend_directwrite`).
    pub const DWRITE: u8 = 1;
    /// CoreText font handle (`aegle-text/backend_coretext`).
    pub const CORETEXT: u8 = 2;
}

/// A shaped glyph and its baseline origin in the scene's local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// Font-specific OpenType glyph index, not a Unicode code point.
    pub id: u16,
    /// Baseline origin; positive y runs down the surface.
    pub position: Point,
}

/// Validated positioned glyphs sharing a font instance and foreground color.
///
/// Font bytes are reference counted and may be memory mapped by the font system.
/// Cloning a font handle does not copy its bytes. This record owns positions and
/// variation coordinates, not a text layout engine or rasterized glyph images.
#[derive(Debug)]
pub struct GlyphRun {
    font: FontSource,
    size: f32,
    color: Color,
    coords: Vec<i16>,
    glyphs: Vec<Glyph>,
    embolden: bool,
    skew: i8,
}

impl GlyphRun {
    /// Creates a run using positive logical font size and normalized F2Dot14 axes.
    ///
    /// Variation coordinates are ordered by the font's axis order and lie within
    /// -16384..=16384. An empty list selects the default instance. Font parsing
    /// and glyph index validation belong to the rasterizer's resource boundary. Panics on a
    /// nonpositive or nonfinite size, nonfinite positions or out-of-range coordinates.
    #[track_caller]
    pub fn new(
        font: impl Into<FontSource>,
        size: f32,
        color: Color,
        coords: Vec<i16>,
        glyphs: Vec<Glyph>,
    ) -> Self {
        if !size.is_finite()
            || glyphs
                .iter()
                .any(|g| !g.position.x.is_finite() || !g.position.y.is_finite())
        {
            crate::fail(SceneError::NonFinite);
        }
        if size <= 0.0 || coords.iter().any(|v| !(-16384..=16384).contains(v)) {
            crate::fail(SceneError::InvalidText);
        }
        Self {
            font: font.into(),
            size,
            color,
            coords,
            glyphs,
            embolden: false,
            skew: 0,
        }
    }

    /// Requests synthetic bold and a slant of `skew` degrees (positive leans right)
    /// for fonts that lack the requested face. Panics unless skew lies within
    /// ±89 degrees.
    #[track_caller]
    pub fn synthesized(mut self, embolden: bool, skew: i8) -> Self {
        if !(-89..=89).contains(&skew) {
            crate::fail(SceneError::InvalidText);
        }
        (self.embolden, self.skew) = (embolden, skew);
        self
    }

    /// Font identity: shared bytes or a platform handle.
    pub fn font(&self) -> &FontSource {
        &self.font
    }
    /// Logical font size before the scene transform.
    pub fn size(&self) -> f32 {
        self.size
    }
    /// Foreground color for alpha glyphs and palette foreground references.
    pub fn color(&self) -> Color {
        self.color
    }
    /// Normalized F2Dot14 variation coordinates in font-axis order.
    pub fn normalized_coords(&self) -> &[i16] {
        &self.coords
    }
    /// Whether the rasterizer should apply synthetic bold.
    pub fn embolden(&self) -> bool {
        self.embolden
    }
    /// Synthetic slant in degrees; zero is upright.
    pub fn skew(&self) -> i8 {
        self.skew
    }
    /// Glyph indices and baseline origins.
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }
    /// Owned array capacity in bytes, excluding the shared font file.
    pub fn allocated_bytes(&self) -> usize {
        self.coords.capacity() * size_of::<i16>() + self.glyphs.capacity() * size_of::<Glyph>()
    }
}
