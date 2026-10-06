//! Bounded decoding of common image formats into shared scene images.
//!
//! PNG is always available through `aegle-glyph`; the `jpeg`, `webp` and `gif`
//! features add those decoders (GIF and WebP yield only a still image: the
//! first GIF frame, and non-animated WebP), and `svg` rasterizes a static SVG
//! without text at a size the caller chooses. Every decoder checks dimensions
//! against the caller's byte budget before allocating pixels, and returns
//! straight (non-premultiplied) sRGB RGBA8, the same as `aegle_glyph::decode_png`.

use aegle_scene::Image;

#[cfg(feature = "gif")]
mod gif_format;
#[cfg(feature = "jpeg")]
mod jpeg;
#[cfg(feature = "svg")]
pub mod svg;
#[cfg(feature = "webp")]
mod webp;

/// Default cap on decoded RGBA bytes, 64 MiB.
pub const DEFAULT_MAX_BYTES: usize = aegle_glyph::DEFAULT_MAX_BYTES;

/// Why an image could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The format is unknown, animated beyond a still frame, or its feature is off.
    Unsupported,
    /// The data is truncated or corrupt.
    Invalid,
    /// The image exceeds [`Image::MAX_EXTENT`] or the byte limit.
    TooLarge,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "unsupported image format",
            Self::Invalid => "truncated or corrupt image",
            Self::TooLarge => "decoded image exceeds the size limit",
        })
    }
}

impl std::error::Error for Error {}

/// Decodes PNG, or JPEG, WebP and GIF when enabled, recognized by signature.
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
/// let image = aegle_image::decode(&png)?;
/// assert_eq!((image.width(), image.height(), image.pixels()), (1, 1, &[255, 0, 0, 255][..]));
/// # Ok::<(), aegle_image::Error>(())
/// ```
pub fn decode(bytes: &[u8]) -> Result<Image, Error> {
    decode_with_limit(bytes, DEFAULT_MAX_BYTES)
}

/// Like [`decode`], rejecting images whose RGBA pixels exceed `max_bytes`.
pub fn decode_with_limit(bytes: &[u8], max_bytes: usize) -> Result<Image, Error> {
    let decoded = match bytes {
        [0x89, b'P', b'N', b'G', ..] => aegle_glyph::decode_png_with_limit(bytes, max_bytes)
            .map(|image| (image.width, image.height, image.pixels))
            .map_err(|error| match error {
                aegle_glyph::DecodeError::TooLarge => Error::TooLarge,
                _ => Error::Invalid,
            }),
        #[cfg(feature = "jpeg")]
        [0xff, 0xd8, 0xff, ..] => jpeg::decode(bytes, max_bytes),
        #[cfg(feature = "webp")]
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => webp::decode(bytes, max_bytes),
        #[cfg(feature = "gif")]
        [b'G', b'I', b'F', b'8', ..] => gif_format::decode(bytes, max_bytes),
        _ => Err(Error::Unsupported),
    }?;
    Image::new(decoded.0, decoded.1, decoded.2).map_err(|_| Error::TooLarge)
}

/// Checks extents and the RGBA byte budget before any pixel allocation.
pub(crate) fn budget(width: u32, height: u32, max_bytes: usize) -> Result<usize, Error> {
    let extent = 1..=Image::MAX_EXTENT;
    if !extent.contains(&width) || !extent.contains(&height) {
        return Err(Error::TooLarge);
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= max_bytes)
        .ok_or(Error::TooLarge)
}
