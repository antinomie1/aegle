//! The Rust glyph rasterizer: Swash outlines plus Skrifa bitmap, COLRv1 and
//! SVG lookups.
//!
//! The module is named `swash_raster`, not `swash`, so that `swash::...` inside
//! the crate keeps meaning the external crate.
//!
//! This is the opt-in `swash` path, kept for hosts with no system text stack
//! and no longer maintained. Linux, Windows and macOS default to their platform
//! rasterizer, so enabling this module on those targets is an explicit choice to
//! replace it — the glyph pixels differ from the system path.
//!
//! `colrv1` and `svg` do **not** imply this module: they are per-glyph
//! representations that Skrifa and resvg decode, and enabling them must not
//! silently swap the platform rasterizer.

use skrifa::{MetadataProvider, instance::Size};
use swash::{
    scale::{ScaleContext, outline::Outline},
    zeno::{Angle, Mask, Origin, Scratch, Transform},
};

use crate::srgb::{encoded, image_size, linear};
use crate::{CacheLimits, Content, FontData, GlyphError, Image, Placement, RasterOptions};

/// Scratch and caches for the Rust outline rasterizer.
pub(crate) struct SwashRasterizer {
    context: ScaleContext,
    outline: Outline,
    scratch: Scratch,
    mask: Vec<u8>,
}

impl SwashRasterizer {
    /// Creates the rasterizer; tables allocate on first use.
    pub fn new() -> Self {
        Self {
            context: ScaleContext::with_max_entries(4),
            outline: Outline::new(),
            scratch: Scratch::new(),
            mask: Vec::new(),
        }
    }

    /// Releases all scratch and cached font proxies.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Rasterizes one glyph of `data` with the Rust stack.
    pub fn rasterize(
        &mut self,
        data: &FontData,
        glyph: u16,
        options: RasterOptions<'_>,
        limits: CacheLimits,
        mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        let font = swash::FontRef::from_index(data.data.data(), data.index as usize)
            .ok_or(GlyphError::InvalidFont)?;
        let metrics = font.metrics(&[]);
        if metrics.units_per_em == 0 || glyph >= metrics.glyph_count {
            return Err(GlyphError::InvalidFont);
        }
        if options.normalized_coords.len() > font.variations().len() {
            return Err(GlyphError::InvalidOptions);
        }
        let parsed = skrifa::FontRef::from_index(data.data.data(), data.index)
            .map_err(|_| GlyphError::InvalidFont)?;
        if let Some(bitmap) = parsed
            .bitmap_strikes()
            .glyph_for_size(Size::new(options.size), glyph.into())
        {
            return crate::bitmap::render(bitmap, metrics.units_per_em, options, limits, reserve);
        }
        // COLRv1 and SVG glyphs are decoded by `crate::color` before the
        // rasterizer is consulted, so only plain outlines and bitmap strikes
        // reach this point.
        let mut scaler = self
            .context
            .builder_with_id(font, [data.data.id(), data.index as u64])
            .size(options.size)
            .hint(options.hint)
            .normalized_coords(options.normalized_coords)
            .build();
        let color = scaler.scale_color_outline_into(glyph, &mut self.outline);
        if !color && !scaler.scale_outline_into(glyph, &mut self.outline) {
            return Err(GlyphError::UnsupportedGlyph);
        }
        if options.embolden {
            let strength = options.size / 32.0;
            self.outline.embolden(strength, strength);
        }
        if options.skew != 0 {
            let angle = Angle::from_degrees(f32::from(options.skew));
            self.outline
                .transform(&Transform::skew(angle, Angle::from_degrees(0.0)));
        }
        let offset = [options.offset[0], -options.offset[1]];
        if self
            .outline
            .points()
            .iter()
            .any(|p| !valid(p.x) || !valid(p.y))
        {
            return Err(GlyphError::ImageBudget);
        }
        if !color {
            let mut mask = Mask::with_scratch(self.outline.path(), &mut self.scratch);
            mask.origin(Origin::BottomLeft)
                .offset(offset)
                .render_offset(offset);
            let mut bytes = 0;
            let mut check = Ok(());
            mask.inspect(|_, w, h| {
                check = image_size(w, h, 1, limits.image_bytes).map(|n| bytes = n);
            });
            check?;
            reserve(bytes)?;
            let mut pixels = vec![0; bytes].into_boxed_slice();
            let placement = mask.render_into(&mut pixels, None);
            return Ok(Image {
                placement: placement.into(),
                content: Content::Mask,
                data: pixels,
            });
        }
        // Union of control-point bounds conservatively contains every color layer.
        let bounds = self.outline.bounds();
        let left = (bounds.min.x + offset[0]).floor() as i32;
        let top = (bounds.max.y + offset[1]).ceil() as i32;
        let width = ((bounds.max.x + offset[0]).ceil() - left as f32) as u32;
        let height = (top as f32 - (bounds.min.y + offset[1]).floor()) as u32;
        let bytes = image_size(width, height, 4, limits.image_bytes)?;
        reserve(bytes)?;
        let mut pixels = vec![0; bytes].into_boxed_slice();
        let palette = font.color_palettes().next();
        for index in 0..self.outline.len() {
            let layer = self.outline.get(index).expect("valid outline layer");
            let mut mask = Mask::with_scratch(layer.path(), &mut self.scratch);
            mask.origin(Origin::BottomLeft)
                .offset(offset)
                .render_offset(offset);
            let mut bytes = 0;
            let mut check = Ok(());
            mask.inspect(|_, w, h| {
                check = image_size(w, h, 1, limits.bitmap_bytes).map(|n| bytes = n);
            });
            check.map_err(|_| GlyphError::BitmapBudget)?;
            if bytes > self.mask.capacity() {
                self.mask = vec![0; bytes];
            } else {
                self.mask.resize(bytes, 0);
                self.mask.fill(0);
            }
            let placement = mask.render_into(&mut self.mask, None);
            let color = layer
                .color_index()
                .and_then(|i| palette.map(|p| p.get(i)))
                .unwrap_or(options.foreground);
            let color = linear(color);
            let x = (placement.left - left) as usize;
            let y = (top - placement.top) as usize;
            for row in 0..placement.height as usize {
                for col in 0..placement.width as usize {
                    let coverage = self.mask[row * placement.width as usize + col] as f32 / 255.0;
                    if coverage == 0.0 {
                        continue;
                    }
                    let dest = ((y + row) * width as usize + x + col) * 4;
                    let dst = &mut pixels[dest..dest + 4];
                    let previous = linear(dst.try_into().expect("RGBA pixel"));
                    let alpha = color[3] * coverage;
                    let out =
                        core::array::from_fn(|c| color[c] * coverage + previous[c] * (1.0 - alpha));
                    dst.copy_from_slice(&encoded(out));
                }
            }
        }
        Ok(Image {
            placement: Placement {
                left,
                top,
                width,
                height,
            },
            content: Content::Color,
            data: pixels,
        })
    }
}

fn valid(v: f32) -> bool {
    v.is_finite() && v.abs() < 16_777_216.0
}
