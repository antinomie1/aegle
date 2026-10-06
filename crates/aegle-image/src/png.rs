//! Bounded PNG decoding to straight sRGB RGBA8, for application images and for
//! the PNG strikes embedded in color fonts. Always available.
use std::io::Cursor;

use crate::Error;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// A decoded image: tightly packed, top-down, straight (non-premultiplied)
/// sRGB RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedImage {
    /// Width in pixels, at most `Image::MAX_EXTENT`.
    pub width: u32,
    /// Height in pixels, at most `Image::MAX_EXTENT`.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub pixels: Vec<u8>,
}

impl DecodedImage {
    /// Wraps the pixels in a shared scene image, ready to draw.
    pub fn into_image(self) -> Result<aegle_scene::Image, Error> {
        aegle_scene::Image::new(self.width, self.height, self.pixels).map_err(|_| Error::TooLarge)
    }
}

/// Decodes a PNG with the [`crate::DEFAULT_MAX_BYTES`] limit.
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
/// let image = aegle_image::png::decode(&png)?;
/// assert_eq!((image.width, image.height, image.pixels), (1, 1, vec![255, 0, 0, 255]));
/// # Ok::<(), aegle_image::Error>(())
/// ```
pub fn decode(data: &[u8]) -> Result<DecodedImage, Error> {
    decode_with_limit(data, crate::DEFAULT_MAX_BYTES)
}

/// Like [`decode`], rejecting images whose RGBA output exceeds `max_bytes`.
/// The limit is checked from the header before any pixel buffer is allocated.
pub fn decode_with_limit(data: &[u8], max_bytes: usize) -> Result<DecodedImage, Error> {
    if !data.starts_with(&SIGNATURE) {
        return Err(Error::Unsupported);
    }
    let mut decoder = decoder(data, max_bytes);
    decoder.set_limits(png::Limits { bytes: max_bytes });
    let reader = decoder.read_info().map_err(png_error)?;
    let (width, height) = (reader.info().width, reader.info().height);
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|&bytes| {
            width <= aegle_scene::Image::MAX_EXTENT
                && height <= aegle_scene::Image::MAX_EXTENT
                && bytes <= max_bytes
        })
        .ok_or(Error::TooLarge)?;
    let mut pixels = vec![0; bytes];
    expand(reader, &mut pixels)?;
    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

/// Decodes a PNG of a known size, such as a font-embedded strike, into a buffer
/// the caller reserved (at least `width × height × 4` bytes). `limit` bounds the
/// decoder's own allocations; a size mismatch is [`Error::Invalid`].
pub fn decode_into(
    data: &[u8],
    expected: [u32; 2],
    limit: usize,
    output: &mut [u8],
) -> Result<(), Error> {
    let reader = decoder(data, limit).read_info().map_err(png_error)?;
    if [reader.info().width, reader.info().height] != expected {
        return Err(Error::Invalid);
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
fn expand(mut reader: png::Reader<Cursor<&[u8]>>, output: &mut [u8]) -> Result<(), Error> {
    let size = reader.output_buffer_size().ok_or(Error::TooLarge)?;
    if size > output.len() {
        return Err(Error::TooLarge);
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
            png::ColorType::Indexed => return Err(Error::Invalid),
        };
        output[i * 4..i * 4 + 4].copy_from_slice(&rgba);
    }
    Ok(())
}

fn png_error(error: png::DecodingError) -> Error {
    match error {
        png::DecodingError::LimitsExceeded => Error::TooLarge,
        _ => Error::Invalid,
    }
}
