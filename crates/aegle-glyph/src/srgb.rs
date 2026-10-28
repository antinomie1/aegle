//! Linear-light helpers for straight sRGB glyph images.
//!
//! Decoded font bitmaps and COLR layers composite in linear premultiplied
//! space; the images Aegle hands to renderers are straight sRGB, so each path
//! converts in and out. These are shared by the fallback rasterizer and by
//! `bitmap.rs`/`colrv1.rs`.

/// Premultiplied linear RGBA while sampling/compositing; public images stay
/// straight sRGB.
#[cfg_attr(not(feature = "swash"), allow(dead_code))]
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

/// Encodes premultiplied linear RGBA back to straight sRGB bytes.
#[cfg_attr(not(feature = "swash"), allow(dead_code))]
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

/// Bytes for `w × h × channels`, refused above `limit`.
#[cfg_attr(not(feature = "swash"), allow(dead_code))]
pub(crate) fn image_size(
    w: u32,
    h: u32,
    channels: usize,
    limit: usize,
) -> Result<usize, crate::GlyphError> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(channels))
        .filter(|n| *n <= limit)
        .ok_or(crate::GlyphError::ImageBudget)
}
