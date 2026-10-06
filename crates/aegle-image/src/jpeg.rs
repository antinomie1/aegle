use zune_jpeg::{
    JpegDecoder,
    zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions},
};

use crate::{Error, budget};

pub(crate) fn decode(bytes: &[u8], max_bytes: usize) -> Result<(u32, u32, Vec<u8>), Error> {
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(aegle_scene::Image::MAX_EXTENT as usize)
        .set_max_height(aegle_scene::Image::MAX_EXTENT as usize);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(|_| Error::Invalid)?;
    let info = decoder.info().ok_or(Error::Invalid)?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let size = budget(width, height, max_bytes)?;
    let mut pixels = vec![0; size];
    decoder
        .decode_into(&mut pixels)
        .map_err(|_| Error::Invalid)?;
    Ok((width, height, pixels))
}
