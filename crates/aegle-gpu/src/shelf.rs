/// Shelf allocator state for one atlas page. A value is the cursor; placing is
/// pure, so callers can test whether a block fits before committing.
#[derive(Clone, Copy, Default)]
pub struct Shelf {
    x: u32,
    y: u32,
    height: u32,
}

impl Shelf {
    /// Places a `width × height` block in an `extent` page, wrapping to a new row.
    /// Returns the advanced cursor and the block's top-left corner, or `None` when
    /// it does not fit. Callers add any border they reserved to `width`/`height`.
    pub fn place(mut self, width: u32, height: u32, extent: [u32; 2]) -> Option<(Self, [u32; 2])> {
        if self.x + width > extent[0] {
            self.x = 0;
            self.y += self.height;
            self.height = 0;
        }
        if width > extent[0] || self.y + height > extent[1] {
            return None;
        }
        let at = [self.x, self.y];
        self.x += width;
        self.height = self.height.max(height);
        Some((self, at))
    }
}
