//! Coverage masks a rasterizer can write and the compositor can read.

use crate::RenderError;

/// A whole-pixel rectangle in mask coordinates, `left..right` by `top..bottom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// First column, inclusive.
    pub left: usize,
    /// First row, inclusive.
    pub top: usize,
    /// Column past the last one, exclusive.
    pub right: usize,
    /// Row past the last one, exclusive.
    pub bottom: usize,
}

impl Area {
    /// An area covering `width` by `height` pixels.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            left: 0,
            top: 0,
            right: width as usize,
            bottom: height as usize,
        }
    }

    /// An area with no pixels.
    pub const EMPTY: Self = Self {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    /// Whether no pixel lies inside.
    pub fn is_empty(&self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    /// Width in pixels.
    pub fn width(&self) -> usize {
        self.right.saturating_sub(self.left)
    }

    /// Height in pixels.
    pub fn height(&self) -> usize {
        self.bottom.saturating_sub(self.top)
    }
}

/// One 8-bit coverage buffer, owned by a rasterizer.
///
/// The compositor clears an area before each rasterization and reads it back
/// afterwards; it never cares how rows are padded, so a backend may use its
/// native layout. Bytes outside the area a fill was asked for are never read, so
/// they may hold the result of an earlier draw.
pub trait Mask: Sized {
    /// Bytes a mask of this size holds, without allocating one.
    ///
    /// The compositor checks its budget with this before allocating, so an
    /// over-budget frame fails without touching memory.
    fn bytes_for(width: u32, height: u32) -> usize;

    /// Allocates a zeroed mask, checking that the backend can hold it.
    fn new(width: u32, height: u32) -> Result<Self, RenderError>;

    /// Width in pixels.
    fn width(&self) -> u32;

    /// Height in pixels.
    fn height(&self) -> u32;

    /// Bytes of pixel storage this mask holds, for budget accounting.
    fn allocated_bytes(&self) -> usize;

    /// Sets the pixels of `area` to zero.
    fn clear(&mut self, area: Area);

    /// The bytes of row `y` starting at column 0.
    ///
    /// Only the columns of the area a fill covered are meaningful; the slice may
    /// extend past them into padding.
    fn row(&self, y: usize) -> &[u8];

    /// The bytes of row `y` starting at column 0, for modification.
    ///
    /// Only pixels the compositor is drawing into may be written; padding is
    /// never read.
    fn row_mut(&mut self, y: usize) -> &mut [u8];
}
