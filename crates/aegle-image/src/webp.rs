use std::io::Cursor;

use image_webp::WebPDecoder;

use crate::{Error, budget};

pub(crate) fn decode(bytes: &[u8], max_bytes: usize) -> Result<(u32, u32, Vec<u8>), Error> {
    let mut decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(|_| Error::Invalid)?;
    if decoder.is_animated() {
        return Err(Error::Unsupported);
    }
    let (width, height) = decoder.dimensions();
    let size = budget(width, height, max_bytes)?;
    let alpha = decoder.has_alpha();
    let mut pixels = vec![0; decoder.output_buffer_size().ok_or(Error::TooLarge)?];
    if pixels.len() > size {
        return Err(Error::TooLarge);
    }
    decoder
        .read_image(&mut pixels)
        .map_err(|_| Error::Invalid)?;
    if alpha {
        return Ok((width, height, pixels));
    }
    let mut rgba = Vec::with_capacity(size);
    for rgb in pixels.as_chunks::<3>().0 {
        rgba.extend([rgb[0], rgb[1], rgb[2], 255]);
    }
    Ok((width, height, rgba))
}
