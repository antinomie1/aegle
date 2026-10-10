use aegle_scene::{Affine, Point};

use crate::{GlyphError, Placement, key::validate_size};

/// Shared font raster size and baseline policy under a scene transform.
///
/// Uses the greater affine column length for raster resolution. Positive,
/// axis-aligned uniform scales hint, snap baselines to whole pixels so horizontal
/// stems stay sharp, and keep quarter-pixel horizontal phases for spacing. Other
/// transforms keep unsnapped origins and use transformed bitmap sampling.
#[derive(Clone, Copy, Debug)]
pub struct RasterTransform {
    transform: Affine,
    scale: f32,
    size: f32,
    hint: bool,
}

impl RasterTransform {
    /// Chooses a validated device raster size from a positive logical font size.
    /// The resulting raster size must fit [`crate::RasterOptions`]'s limits.
    pub fn new(transform: Affine, size: f32) -> Result<Self, GlyphError> {
        let [a, b, c, d, _, _] = transform.coefficients();
        let scale = a.hypot(b).max(c.hypot(d));
        let size = size * scale;
        validate_size(size)?;
        Ok(Self {
            transform,
            scale,
            size,
            hint: b == 0.0 && c == 0.0 && a == d && a > 0.0,
        })
    }

    /// Device pixels per em to pass to the glyph rasterizer.
    pub fn size(self) -> f32 {
        self.size
    }

    /// Whether glyphs use hinting and pixel-aligned bitmap placement.
    /// False requires filtered sampling of the resulting image transform.
    pub fn hint(self) -> bool {
        self.hint
    }

    /// Maps a local baseline and applies the shared pixel-snapping policy.
    /// Rejects nonfinite or device coordinates beyond ±1,048,576.
    pub fn origin(self, point: Point) -> Result<GlyphOrigin, GlyphError> {
        let mut point = self.transform.map_point(point);
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > 1_048_576.0
            || point.y.abs() > 1_048_576.0
        {
            return Err(GlyphError::Coordinates);
        }
        if self.hint {
            point.x = (point.x * 4.0).round() * 0.25;
            point.y = point.y.round();
        }
        Ok(GlyphOrigin {
            raster: self,
            point,
        })
    }
}

/// Mapped baseline and raster phase for one glyph; it owns no image or font.
#[derive(Clone, Copy, Debug)]
pub struct GlyphOrigin {
    raster: RasterTransform,
    point: Point,
}

impl GlyphOrigin {
    /// Fractional raster offset in `[0, 1)`; unsnapped transforms use zero phase.
    pub fn offset(self) -> [f32; 2] {
        if self.raster.hint {
            [
                self.point.x - self.point.x.floor(),
                self.point.y - self.point.y.floor(),
            ]
        } else {
            [0.0; 2]
        }
    }

    /// Maps image pixel coordinates to the device from baseline-relative placement.
    /// The renderer still clips and validates the image's mapped pixel bounds.
    pub fn image_transform(self, placement: Placement) -> Result<Affine, GlyphError> {
        if self.raster.hint {
            return Ok(Affine::translation(
                self.point.x.floor() + placement.left as f32,
                self.point.y.floor() - placement.top as f32,
            ));
        }
        let [a, b, c, d, _, _] = self.raster.transform.coefficients();
        let scale = self.raster.scale;
        let left = placement.left as f32;
        let top = -(placement.top as f32);
        Affine::new([
            a / scale,
            b / scale,
            c / scale,
            d / scale,
            self.point.x + (a * left + c * top) / scale,
            self.point.y + (b * left + d * top) / scale,
        ])
        .map_err(|_| GlyphError::Coordinates)
    }
}
