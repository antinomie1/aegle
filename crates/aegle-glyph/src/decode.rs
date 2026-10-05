//! Bounded PNG decoding to straight sRGB RGBA8, for application images and for
//! the PNG strikes embedded in color fonts.
use std::io::Cursor;

use crate::GlyphError;

/// Largest accepted width or height, equal to `aegle_scene::Image::MAX_EXTENT`.
pub const MAX_EXTENT: u32 = 16_384;
/// Default cap on decoded RGBA bytes (64 MiB, about 4096 × 4096 pixels).
pub const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// A decoded image: tightly packed, top-down, straight (non-premultiplied)
/// sRGB RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedImage {
    /// Width in pixels, at most [`MAX_EXTENT`].
    pub width: u32,
    /// Height in pixels, at most [`MAX_EXTENT`].
    pub height: u32,
    /// `width × height × 4` bytes.
    pub pixels: Vec<u8>,
}

#[cfg(feature = "scene")]
impl DecodedImage {
    /// Wraps the pixels in a shared scene image, ready to draw.
    pub fn into_image(self) -> Result<aegle_scene::Image, DecodeError> {
        aegle_scene::Image::new(self.width, self.height, self.pixels)
            .map_err(|_| DecodeError::TooLarge)
    }
}

/// Why an image could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes are not a PNG file; only PNG is supported.
    NotPng,
    /// The PNG is truncated or corrupt.
    Invalid,
    /// The image exceeds [`MAX_EXTENT`] or the byte limit.
    TooLarge,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NotPng => "not a PNG image",
            Self::Invalid => "truncated or corrupt PNG image",
            Self::TooLarge => "decoded image exceeds the size limit",
        })
    }
}

impl std::error::Error for DecodeError {}

/// Decodes a PNG with the [`DEFAULT_MAX_BYTES`] limit.
///
/// Every color type and bit depth becomes RGBA8: palettes and `tRNS` transparency
/// expand, 16-bit channels reduce to 8, and interlaced images decode normally.
/// Gamma and ICC chunks are ignored, so pixels are taken as sRGB. Animated PNGs
/// yield their default image.
///
/// ```
/// // A 1 × 1 opaque red PNG.
/// let png = [
///     0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
///     0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
///     0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
///     0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
///     0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
/// ];
/// let image = aegle_glyph::decode_png(&png)?;
/// assert_eq!((image.width, image.height, image.pixels), (1, 1, vec![255, 0, 0, 255]));
/// # Ok::<(), aegle_glyph::DecodeError>(())
/// ```
pub fn decode_png(data: &[u8]) -> Result<DecodedImage, DecodeError> {
    decode_png_with_limit(data, DEFAULT_MAX_BYTES)
}

/// Like [`decode_png`], rejecting images whose RGBA output exceeds `max_bytes`.
/// The limit is checked from the header before any pixel buffer is allocated.
pub fn decode_png_with_limit(data: &[u8], max_bytes: usize) -> Result<DecodedImage, DecodeError> {
    if !data.starts_with(&SIGNATURE) {
        return Err(DecodeError::NotPng);
    }
    let mut decoder = decoder(data, max_bytes);
    decoder.set_limits(png::Limits { bytes: max_bytes });
    let reader = decoder
        .read_info()
        .map_err(|e| to_decode_error(png_error(e)))?;
    let (width, height) = (reader.info().width, reader.info().height);
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|&bytes| width <= MAX_EXTENT && height <= MAX_EXTENT && bytes <= max_bytes)
        .ok_or(DecodeError::TooLarge)?;
    let mut pixels = vec![0; bytes];
    expand(reader, &mut pixels).map_err(to_decode_error)?;
    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

/// Decodes a PNG straight into a scene image with the default limit.
#[cfg(feature = "scene")]
pub fn decode_image(data: &[u8]) -> Result<aegle_scene::Image, DecodeError> {
    decode_png(data)?.into_image()
}

/// Decodes a font-embedded PNG whose size the font declared, into a buffer the
/// caller reserved. `limit` bounds the decoder's own allocations.
pub(crate) fn decode_png_into(
    data: &[u8],
    expected: [u32; 2],
    limit: usize,
    output: &mut [u8],
) -> Result<(), GlyphError> {
    let reader = decoder(data, limit).read_info().map_err(png_error)?;
    if [reader.info().width, reader.info().height] != expected {
        return Err(GlyphError::InvalidFont);
    }
    expand(reader, output)
}

fn decoder(data: &[u8], limit: usize) -> png::Decoder<Cursor<&[u8]>> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_limits(png::Limits { bytes: limit });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder
}

/// Reads the frame into `output` (at least RGBA-sized) and widens it to RGBA in
/// place, back to front.
fn expand(mut reader: png::Reader<Cursor<&[u8]>>, output: &mut [u8]) -> Result<(), GlyphError> {
    let size = reader
        .output_buffer_size()
        .ok_or(GlyphError::BitmapBudget)?;
    if size > output.len() {
        return Err(GlyphError::BitmapBudget);
    }
    let info = reader.next_frame(output).map_err(png_error)?;
    let channels = info.color_type.samples();
    for i in (0..info.width as usize * info.height as usize).rev() {
        let p = i * channels;
        let rgba = match info.color_type {
            png::ColorType::Rgba => continue,
            png::ColorType::Rgb => [output[p], output[p + 1], output[p + 2], 255],
            png::ColorType::Grayscale => [output[p], output[p], output[p], 255],
            png::ColorType::GrayscaleAlpha => [output[p], output[p], output[p], output[p + 1]],
            // EXPAND converts palettes before they reach this point.
            png::ColorType::Indexed => return Err(GlyphError::InvalidFont),
        };
        output[i * 4..i * 4 + 4].copy_from_slice(&rgba);
    }
    Ok(())
}

fn png_error(error: png::DecodingError) -> GlyphError {
    match error {
        png::DecodingError::LimitsExceeded => GlyphError::BitmapBudget,
        _ => GlyphError::InvalidFont,
    }
}

fn to_decode_error(error: GlyphError) -> DecodeError {
    match error {
        GlyphError::BitmapBudget => DecodeError::TooLarge,
        _ => DecodeError::Invalid,
    }
}
