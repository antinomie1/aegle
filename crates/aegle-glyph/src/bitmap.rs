use skrifa::bitmap::{BitmapData, BitmapGlyph, Origin};

use crate::raster::{encoded, image_size, linear};
use crate::{CacheLimits, Content, GlyphError, Image, Placement, RasterOptions};

pub(crate) fn render(
    bitmap: BitmapGlyph<'_>,
    upem: u16,
    options: RasterOptions<'_>,
    limits: CacheLimits,
    mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
) -> Result<Image, GlyphError> {
    if bitmap.ppem_x <= 0.0 || bitmap.ppem_y <= 0.0 || upem == 0 {
        return Err(GlyphError::InvalidFont);
    }
    let color = !matches!(bitmap.data, BitmapData::Mask(_));
    let channels = if color { 4 } else { 1 };
    let source_bytes = image_size(bitmap.width, bitmap.height, channels, limits.bitmap_bytes)
        .map_err(|_| GlyphError::BitmapBudget)?;
    let sx = options.size / bitmap.ppem_x;
    let sy = options.size / bitmap.ppem_y;
    let x = bitmap.bearing_x * options.size / upem as f32
        + bitmap.inner_bearing_x * sx
        + options.offset[0];
    let mut y = bitmap.bearing_y * options.size / upem as f32 + bitmap.inner_bearing_y * sy
        - options.offset[1];
    if bitmap.placement_origin == Origin::BottomLeft {
        y += bitmap.height as f32 * sy;
    }
    if !x.is_finite() || !y.is_finite() || x.abs() >= 16_777_216.0 || y.abs() >= 16_777_216.0 {
        return Err(GlyphError::ImageBudget);
    }
    let left = x.floor() as i32;
    let top = y.ceil() as i32;
    let width = ((x + bitmap.width as f32 * sx).ceil() - left as f32) as u32;
    let height = (top as f32 - (y - bitmap.height as f32 * sy).floor()) as u32;
    let bytes = image_size(width, height, channels, limits.image_bytes)?;
    reserve(bytes)?;
    let mut source = vec![0; source_bytes];
    match bitmap.data {
        BitmapData::Mask(mask) => mask
            .decode_to_slice(bitmap.width, bitmap.height, &mut source)
            .map_err(|_| GlyphError::InvalidFont)?,
        BitmapData::Bgra(data) => {
            if data.len() != source.len() {
                return Err(GlyphError::InvalidFont);
            }
            for (src, dst) in data.chunks_exact(4).zip(source.chunks_exact_mut(4)) {
                let a = src[3] as u32;
                for c in 0..3 {
                    dst[c] = if a == 0 {
                        0
                    } else {
                        ((src[2 - c] as u32 * 255 + a / 2) / a).min(255) as u8
                    };
                }
                dst[3] = src[3];
            }
        }
        BitmapData::Png(data) => aegle_image::png::decode_into(
            data,
            [bitmap.width, bitmap.height],
            limits.bitmap_bytes,
            &mut source,
        )
        .map_err(|error| match error {
            aegle_image::Error::TooLarge => GlyphError::BitmapBudget,
            _ => GlyphError::InvalidFont,
        })?,
    }
    let mut pixels = vec![0; bytes].into_boxed_slice();
    for row in 0..height {
        let py = ((row as f32 + 0.5 - (top as f32 - y)) / sy) - 0.5;
        for col in 0..width {
            let px = ((col as f32 + 0.5 - (x - left as f32)) / sx) - 0.5;
            // Tiny sizes can produce huge or nonfinite inverse coordinates.
            // Cull outside bilinear support before converting to pixel indices.
            if !(px > -1.0 && py > -1.0 && px < bitmap.width as f32 && py < bitmap.height as f32) {
                continue;
            }
            let ix = px.floor() as i32;
            let iy = py.floor() as i32;
            let fx = px - ix as f32;
            let fy = py - iy as f32;
            let mut value = [0.0; 4];
            for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
                    let (xx, yy) = (ix + dx, iy + dy);
                    if xx < 0 || yy < 0 || xx >= bitmap.width as i32 || yy >= bitmap.height as i32 {
                        continue;
                    }
                    let i = (yy as usize * bitmap.width as usize + xx as usize) * channels;
                    let sample = if color {
                        linear(source[i..i + 4].try_into().expect("RGBA pixel"))
                    } else {
                        [0.0, 0.0, 0.0, source[i] as f32 / 255.0]
                    };
                    for c in 0..4 {
                        value[c] += sample[c] * wx * wy;
                    }
                }
            }
            let i = (row as usize * width as usize + col as usize) * channels;
            if color {
                pixels[i..i + 4].copy_from_slice(&encoded(value));
            } else {
                pixels[i] = (value[3] * 255.0).round() as u8;
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
        content: if color { Content::Color } else { Content::Mask },
        data: pixels,
    })
}
