/// An icon as straight (not premultiplied) RGBA8 pixels, row by row from the
/// top; window systems scale it to the sizes they show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: &'a [u8],
}

impl Icon<'_> {
    /// Whether the icon has pixels and `rgba` holds exactly all of them.
    pub fn is_valid(&self) -> bool {
        let bytes = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4));
        self.width != 0 && self.height != 0 && bytes == Some(self.rgba.len())
    }
}
