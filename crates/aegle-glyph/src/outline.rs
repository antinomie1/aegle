//! Shared path → coverage rasterization for platform glyph outlines.
//!
//! GDI's `GetGlyphOutlineW(GGO_NATIVE)` and CoreText's
//! `CTFontCreatePathForGlyph` both hand back an outline in font units scaled to
//! the requested pixel size, with y pointing up from the baseline. This module
//! turns such an outline into the same grayscale [`Image`] the FreeType path
//! produces: tight 8-bit coverage, top-down, baseline-relative placement.
//!
//! The sink applies the two synthetic style operations the scene can request —
//! a slant about the baseline origin and a bold expansion — before rasterizing,
//! so both platform backends get identical geometry from identical input.

use tiny_skia::{FillRule, Mask, Path, PathBuilder, PathStroker, Stroke, Transform};

use crate::{Content, GlyphError, Image, Placement};

/// Largest outline coordinate magnitude, matching the renderers' device range.
const MAX_COORD: f32 = 1_048_576.0;

/// Builder for one platform glyph outline in y-up font space.
///
/// Feed every contour with [`Self::move_to`], [`Self::line_to`],
/// [`Self::quad_to`], [`Self::cubic_to`] and [`Self::close`], then call
/// [`Self::rasterize`]. Points are in pixels relative to the glyph origin, y
/// increasing upward; TrueType quadratic and CFF cubic segments are accepted.
pub struct OutlineBuilder {
    builder: PathBuilder,
    skew: f32,
    embolden: f32,
    min: [f32; 2],
    max: [f32; 2],
    open: bool,
    invalid: bool,
}

impl OutlineBuilder {
    /// Starts an outline for a glyph rasterized at `size` pixels per em.
    ///
    /// `embolden` expands the finished outline by `size / 32` pixels in total,
    /// half on each side, like `FT_Outline_Embolden`. `skew` slants the outline
    /// about the baseline origin by that many degrees, positive leaning right.
    pub fn new(size: f32, embolden: bool, skew: i8) -> Self {
        Self {
            builder: PathBuilder::new(),
            skew: if skew == 0 {
                0.0
            } else {
                f64::from(skew).to_radians().tan() as f32
            },
            embolden: match embolden {
                true => size / 32.0,
                false => 0.0,
            },
            min: [f32::INFINITY; 2],
            max: [f32::NEG_INFINITY; 2],
            open: false,
            invalid: false,
        }
    }

    /// Whether any contour has been started.
    pub fn is_empty(&self) -> bool {
        self.min[0] == f32::INFINITY
    }

    /// Starts a contour at `(x, y)` in y-up font space.
    pub fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.builder.move_to(x, y);
        self.open = true;
    }

    /// Adds a straight segment to `(x, y)`.
    pub fn line_to(&mut self, x: f32, y: f32) {
        if !self.open {
            return;
        }
        let (x, y) = self.point(x, y);
        self.builder.line_to(x, y);
    }

    /// Adds a quadratic segment with control point `(cx, cy)` to `(x, y)`.
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if !self.open {
            return;
        }
        let (cx, cy) = self.point(cx, cy);
        let (x, y) = self.point(x, y);
        self.builder.quad_to(cx, cy, x, y);
    }

    /// Adds a cubic segment with controls `(c1x, c1y)` and `(c2x, c2y)` to `(x, y)`.
    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        if !self.open {
            return;
        }
        let (c1x, c1y) = self.point(c1x, c1y);
        let (c2x, c2y) = self.point(c2x, c2y);
        let (x, y) = self.point(x, y);
        self.builder.cubic_to(c1x, c1y, c2x, c2y, x, y);
    }

    /// Closes the current contour.
    pub fn close(&mut self) {
        if self.open {
            self.builder.close();
            self.open = false;
        }
    }

    /// Applies the slant and records the point's conservative bounds.
    fn point(&mut self, x: f32, y: f32) -> (f32, f32) {
        let x = x + self.skew * y;
        if !x.is_finite() || !y.is_finite() {
            self.invalid = true;
            return (x, y);
        }
        self.min = [self.min[0].min(x), self.min[1].min(y)];
        self.max = [self.max[0].max(x), self.max[1].max(y)];
        (x, y)
    }

    /// Rasterizes the outline into a coverage mask.
    ///
    /// `offset` is the fractional baseline phase in device pixels, positive x to
    /// the right and positive y downward, as in [`crate::RasterOptions::offset`].
    /// `reserve` is asked for the image bytes before they are allocated. An
    /// empty outline yields an empty image with zero placement, as FreeType
    /// does for spaces.
    pub fn rasterize(
        self,
        offset: [f32; 2],
        mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        if self.invalid {
            return Err(GlyphError::Coordinates);
        }
        if self.is_empty() {
            reserve(0)?;
            return Ok(Image {
                placement: Placement::default(),
                content: Content::Mask,
                data: Vec::new().into_boxed_slice(),
            });
        }
        if self.invalid
            || ![self.min[0], self.min[1], self.max[0], self.max[1]]
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= MAX_COORD)
        {
            return Err(GlyphError::Coordinates);
        }
        // y flips here: tiny-skia's coverage raster is y-down. A synthetic bold
        // stroke extends half its width beyond the control-point bounds.
        let grow = self.embolden / 2.0;
        let left = (self.min[0] - grow + offset[0]).floor();
        let top = (self.max[1] + grow + offset[1]).ceil();
        let right = (self.max[0] + grow + offset[0]).ceil();
        let bottom = (self.min[1] - grow + offset[1]).floor();
        let width = (right - left).max(0.0) as u32;
        let height = (top - bottom).max(0.0) as u32;
        if width == 0 || height == 0 {
            reserve(0)?;
            return Ok(Image {
                placement: Placement::default(),
                content: Content::Mask,
                data: Vec::new().into_boxed_slice(),
            });
        }
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .ok_or(GlyphError::ImageBudget)?;
        reserve(bytes)?;
        let mut mask = Mask::new(width, height).ok_or(GlyphError::ImageBudget)?;
        let path = self.builder.finish().ok_or(GlyphError::UnsupportedGlyph)?;
        // Font space is y-up from the baseline; the mask is y-down from `top`.
        let transform = Transform::from_row(1.0, 0.0, 0.0, -1.0, offset[0] - left, top - offset[1]);
        let mut fill = |path: &Path| {
            mask.fill_path(path, FillRule::Winding, true, transform);
        };
        fill(&path);
        if self.embolden > 0.0 {
            // Union of the fill and a stroke grows every side by half the width.
            let mut stroker = PathStroker::new();
            let stroke = Stroke {
                width: self.embolden,
                ..Stroke::default()
            };
            if let Some(outline) = stroker.stroke(&path, &stroke, 1.0) {
                fill(&outline);
            }
        }
        Ok(Image {
            placement: Placement {
                left: left as i32,
                top: top as i32,
                width,
                height,
            },
            content: Content::Mask,
            data: mask.data().to_vec().into_boxed_slice(),
        })
    }
}
