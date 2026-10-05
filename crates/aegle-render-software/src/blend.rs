#[cfg(feature = "text")]
pub(crate) use aegle_types::color_math::{encoded_rgba, linear_rgba};
use aegle_types::{Color, color_math::SrgbTransfer};

/// Prepared solid paint; reuse across all covered pixels of one primitive.
pub(crate) struct Solid {
    rgba: [u8; 4],
    linear: [f32; 3],
    alpha: f32,
    transfer: &'static SrgbTransfer,
}

impl Solid {
    pub(crate) fn new(color: Color) -> Self {
        let rgba = color.to_rgba();
        let transfer = SrgbTransfer::get();
        Self {
            rgba,
            linear: std::array::from_fn(|i| transfer.decode(rgba[i] as f32 / 255.0)),
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
            let destination = self.transfer.decode(*channel as f32 * unpremultiply);
            let linear =
                (self.linear[i] * source_alpha + destination * destination_weight) * normalize;
            let encoded = self.transfer.encode(linear);
            *channel = ((encoded * output_alpha * 255.0).round() as u8).min(alpha);
        }
        dst[3] = alpha;
    }
}
