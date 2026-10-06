use std::{io::Cursor, num::NonZeroU64};

use gif::{ColorOutput, DecodeOptions, MemoryLimit};

use crate::{Error, budget};

/// The first frame composed onto a transparent logical screen.
pub(crate) fn decode(bytes: &[u8], max_bytes: usize) -> Result<(u32, u32, Vec<u8>), Error> {
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    options.set_memory_limit(MemoryLimit::Bytes(
        NonZeroU64::new(max_bytes.max(1) as u64).expect("nonzero"),
    ));
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(|_| Error::Invalid)?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let size = budget(width, height, max_bytes)?;
    let frame = decoder
        .read_next_frame()
        .map_err(|_| Error::Invalid)?
        .ok_or(Error::Invalid)?;
    let mut screen = vec![0; size];
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let frame_width = usize::from(frame.width);
    for (row, line) in frame.buffer.chunks_exact(frame_width * 4).enumerate() {
        let y = top + row;
        if y >= height as usize {
            break;
        }
        let columns = frame_width.min((width as usize).saturating_sub(left));
        let start = (y * width as usize + left) * 4;
        screen[start..start + columns * 4].copy_from_slice(&line[..columns * 4]);
    }
    Ok((width, height, screen))
}
