//! Shared sRGB transfer tables for linear-light rendering and animation.
//!
//! This optional module requires `std`. A process-wide pair of 1025-entry tables
//! occupies 8200 bytes and initializes once without heap allocation. Normalized
//! transfer samples interpolate adjacent entries instead of evaluating powers.

use std::sync::OnceLock;

const INTERVALS: usize = 1024;

/// Shared, interpolated sRGB decoding and encoding tables.
///
/// Hold the reference returned by [`Self::get`] across a rendering batch to avoid
/// repeating the initialization check for each pixel.
pub struct SrgbTransfer {
    decode: [f32; INTERVALS + 1],
    encode: [f32; INTERVALS + 1],
}

impl SrgbTransfer {
    /// Gets the process-wide tables, initializing them on the first call.
    pub fn get() -> &'static Self {
        static TABLES: OnceLock<SrgbTransfer> = OnceLock::new();
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

    /// Decodes a normalized sRGB channel into linear light.
    ///
    /// The caller supplies a finite channel in `0..=1`. Renderer inputs already
    /// establish this range when validating pixel storage and coverage.
    #[inline]
    pub fn decode(&self, value: f32) -> f32 {
        sample(&self.decode, value)
    }

    /// Encodes a normalized linear-light channel into sRGB.
    ///
    /// The caller supplies a finite channel in `0..=1`; this low-level transfer
    /// function does not clamp already-normalized channels on every pixel.
    #[inline]
    pub fn encode(&self, value: f32) -> f32 {
        sample(&self.encode, value)
    }
}

#[inline]
fn sample(table: &[f32; INTERVALS + 1], value: f32) -> f32 {
    let position = value * INTERVALS as f32;
    let index = (position as usize).min(INTERVALS - 1);
    table[index] + (table[index + 1] - table[index]) * (position - index as f32)
}

/// Converts unpremultiplied sRGB bytes into premultiplied linear-light RGBA.
#[inline]
pub fn linear_rgba(rgba: [u8; 4]) -> [f32; 4] {
    let table = SrgbTransfer::get();
    let a = rgba[3] as f32 / 255.0;
    let mut result = rgba.map(|value| table.decode(value as f32 / 255.0) * a);
    result[3] = a;
    result
}

/// Converts premultiplied linear-light RGBA into unpremultiplied sRGB bytes.
///
/// The input has finite alpha in `0..=1` and color channels in `0..=alpha`.
/// Zero alpha produces transparent black. Rounding excursions beyond the color
/// range are clamped while unpremultiplying.
#[inline]
pub fn encoded_rgba(linear: [f32; 4]) -> [u8; 4] {
    if linear[3] <= 0.0 {
        return [0; 4];
    }
    let table = SrgbTransfer::get();
    let mut result = linear
        .map(|value| (table.encode((value / linear[3]).clamp(0.0, 1.0)) * 255.0).round() as u8);
    result[3] = (linear[3] * 255.0).round() as u8;
    result
}
