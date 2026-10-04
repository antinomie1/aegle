use crate::{Color, Point, SceneError};
use alloc::vec::Vec;

pub use linebender_resource_handle::{Blob, FontData};

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
    font: FontData,
    size: f32,
    color: Color,
    coords: Vec<i16>,
    glyphs: Vec<Glyph>,
}

impl GlyphRun {
    /// Creates a run using positive logical font size and normalized F2Dot14 axes.
    ///
    /// Variation coordinates are ordered by the font's axis order and lie within
    /// -16384..=16384. An empty list selects the default instance. Font parsing
    /// and glyph index validation belong to the rasterizer's resource boundary.
    pub fn new(
        font: FontData,
        size: f32,
        color: Color,
        coords: Vec<i16>,
        glyphs: Vec<Glyph>,
    ) -> Result<Self, SceneError> {
        if !size.is_finite()
            || glyphs
                .iter()
                .any(|g| !g.position.x.is_finite() || !g.position.y.is_finite())
        {
            return Err(SceneError::NonFinite);
        }
        if size <= 0.0 || coords.iter().any(|v| !(-16384..=16384).contains(v)) {
            return Err(SceneError::InvalidText);
        }
        Ok(Self {
            font,
            size,
            color,
            coords,
            glyphs,
        })
    }

    /// Shared font file and face index.
    pub fn font(&self) -> &FontData {
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
    /// Glyph indices and baseline origins.
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }
    /// Owned array capacity in bytes, excluding the shared font file.
    pub fn allocated_bytes(&self) -> usize {
        self.coords.capacity() * size_of::<i16>() + self.glyphs.capacity() * size_of::<Glyph>()
    }
}
