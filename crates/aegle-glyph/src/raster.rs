use skrifa::{MetadataProvider, instance::Size, raw::TableProvider};
use swash::{
    scale::{ScaleContext, outline::Outline},
    zeno::{Angle, Mask, Origin, Scratch, Transform},
};

use crate::{CacheLimits, Content, FontData, GlyphError, Image, Placement, RasterOptions};

pub(crate) struct Rasterizer {
    context: ScaleContext,
    outline: Outline,
    scratch: Scratch,
    mask: Vec<u8>,
}

impl Rasterizer {
    pub fn new() -> Self {
        Self {
            context: ScaleContext::with_max_entries(4),
            outline: Outline::new(),
            scratch: Scratch::new(),
            mask: Vec::new(),
        }
    }

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
        if let Some(color) = parsed
            .color_glyphs()
            .get(glyph.into())
            .filter(|c| matches!(c.format(), skrifa::color::ColorGlyphFormat::ColrV1))
        {
            #[cfg(feature = "colrv1")]
            {
                let palette = font.color_palettes().next();
                return crate::colrv1::render(
                    crate::colrv1::Request {
                        glyph: color,
                        outlines: parsed.outline_glyphs(),
                        coords: options.normalized_coords,
                        units_per_em: f32::from(metrics.units_per_em),
                        size: options.size,
                        offset: options.offset,
                        palette: palette
                            .map(|p| (0..p.len()).map(|i| p.get(i)).collect())
                            .unwrap_or_default(),
                        foreground: options.foreground,
                        max_bytes: limits.image_bytes,
                    },
                    reserve,
                );
            }
            #[cfg(not(feature = "colrv1"))]
            {
                let _ = color;
                return Err(GlyphError::UnsupportedGlyph);
            }
        }
        if let Ok(svg) = parsed.svg() {
            if let Some(document) = svg
                .glyph_data(glyph.into())
                .map_err(|_| GlyphError::InvalidFont)?
            {
                #[cfg(feature = "svg")]
                return crate::svg::render(
                    crate::svg::Request {
                        data: document,
                        glyph,
                        units_per_em: f32::from(metrics.units_per_em),
                        size: options.size,
                        offset: options.offset,
                        max_bytes: limits.image_bytes,
                    },
                    reserve,
                );
                #[cfg(not(feature = "svg"))]
                {
                    let _ = document;
                    return Err(GlyphError::UnsupportedGlyph);
                }
            }
        }
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
                placement,
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

pub(crate) fn image_size(
    w: u32,
    h: u32,
    channels: usize,
    limit: usize,
) -> Result<usize, GlyphError> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(channels))
        .filter(|n| *n <= limit)
        .ok_or(GlyphError::ImageBudget)
}

// Premultiplied linear RGBA while sampling/compositing; public images stay straight sRGB.
pub(crate) fn linear(rgba: [u8; 4]) -> [f32; 4] {
    let a = rgba[3] as f32 / 255.0;
    let decode = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    [
        decode(rgba[0]) * a,
        decode(rgba[1]) * a,
        decode(rgba[2]) * a,
        a,
    ]
}

pub(crate) fn encoded(rgba: [f32; 4]) -> [u8; 4] {
    if rgba[3] <= 0.0 {
        return [0; 4];
    }
    let encode = |v: f32| {
        let v = (v / rgba[3]).clamp(0.0, 1.0);
        ((if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }) * 255.0)
            .round() as u8
    };
    [
        encode(rgba[0]),
        encode(rgba[1]),
        encode(rgba[2]),
        (rgba[3] * 255.0).round() as u8,
    ]
}
