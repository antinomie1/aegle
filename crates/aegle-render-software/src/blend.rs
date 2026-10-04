use aegle_types::Color;
use std::sync::OnceLock;

const INTERVALS: usize = 1024;

// The surface stores RGBA bytes with sRGB-encoded RGB premultiplied by alpha.
// Compositing itself occurs in linear light. Interpolated transfer tables avoid
// per-pixel powers, including for non-byte unpremultiplied destination channels.
struct Transfer {
    decode: [f32; INTERVALS + 1],
    encode: [f32; INTERVALS + 1],
}

impl Transfer {
    fn get() -> &'static Self {
        static TABLES: OnceLock<Transfer> = OnceLock::new();
        TABLES.get_or_init(|| Self {
            decode: std::array::from_fn(|i| {
                let value = i as f32 / INTERVALS as f32;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            }),
            encode: std::array::from_fn(|i| {
                let value = i as f32 / INTERVALS as f32;
                if value <= 0.0031308 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1.0 / 2.4) - 0.055
                }
            }),
        })
    }
}

#[inline]
fn sample(table: &[f32; INTERVALS + 1], value: f32) -> f32 {
    let position = value * INTERVALS as f32;
    let index = (position as usize).min(INTERVALS - 1);
    table[index] + (table[index + 1] - table[index]) * (position - index as f32)
}

/// Prepared solid paint; reuse across all covered pixels of one primitive.
pub(crate) struct Solid {
    rgba: [u8; 4],
    linear: [f32; 3],
    alpha: f32,
    transfer: &'static Transfer,
}

impl Solid {
    pub(crate) fn new(color: Color) -> Self {
        let rgba = color.to_rgba();
        let transfer = Transfer::get();
        Self {
            rgba,
            linear: std::array::from_fn(|i| sample(&transfer.decode, rgba[i] as f32 / 255.0)),
            alpha: rgba[3] as f32 / 255.0,
            transfer,
        }
    }

    /// Blends onto one valid premultiplied RGBA pixel (exactly four bytes).
    /// Coverage is linear geometric coverage, independent of source opacity.
    #[inline]
    pub(crate) fn blend(&self, dst: &mut [u8], coverage: u8) {
        if coverage == 0 || self.rgba[3] == 0 {
            return;
        }
        if coverage == 255 && self.rgba[3] == 255 {
            dst.copy_from_slice(&self.rgba);
            return;
        }
        let source_alpha = self.alpha * (coverage as f32 / 255.0);
        if dst[3] == 0 {
            let alpha = (source_alpha * 255.0).round() as u8;
            for (i, channel) in dst[..3].iter_mut().enumerate() {
                *channel = ((self.rgba[i] as f32 * source_alpha).round() as u8).min(alpha);
            }
            dst[3] = alpha;
            return;
        }
        let destination_weight = (dst[3] as f32 / 255.0) * (1.0 - source_alpha);
        let output_alpha = source_alpha + destination_weight;
        let alpha = (output_alpha * 255.0).round() as u8;
        let unpremultiply = 1.0 / dst[3] as f32;
        let normalize = 1.0 / output_alpha;
        for (i, channel) in dst[..3].iter_mut().enumerate() {
            let destination = sample(&self.transfer.decode, *channel as f32 * unpremultiply);
            let linear =
                (self.linear[i] * source_alpha + destination * destination_weight) * normalize;
            let encoded = sample(&self.transfer.encode, linear);
            *channel = ((encoded * output_alpha * 255.0).round() as u8).min(alpha);
        }
        dst[3] = alpha;
    }
}
