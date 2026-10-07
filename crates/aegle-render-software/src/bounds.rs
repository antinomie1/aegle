//! Whole-pixel rectangles of a surface that drawing reads and writes.

use aegle_types::Rect;
use tiny_skia::Path;

use crate::Surface;

/// Pixel columns `left..right` of rows `top..bottom`.
#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub(crate) left: usize,
    pub(crate) top: usize,
    pub(crate) right: usize,
    pub(crate) bottom: usize,
}

impl Bounds {
    pub(crate) const EMPTY: Self = Self {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    /// The pixels of a valid in-range rectangle whose edges are whole device
    /// pixels; `None` for any other rectangle.
    pub(crate) fn aligned(rect: Rect, surface: &Surface<'_>) -> Option<Self> {
        let edges = [
            rect.origin.x,
            rect.origin.y,
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        ];
        let whole = edges
            .iter()
            .all(|e| e.fract() == 0.0 && e.abs() <= 1_048_576.0);
        if !whole || rect.size.width < 0.0 || rect.size.height < 0.0 {
            return None;
        }
        let clamp = |value: f32, limit: u32| value.clamp(0.0, limit as f32) as usize;
        let bounds = Self {
            left: clamp(edges[0], surface.width),
            top: clamp(edges[1], surface.height),
            right: clamp(edges[2], surface.width),
            bottom: clamp(edges[3], surface.height),
        };
        Some(if bounds.is_empty() {
            Self::EMPTY
        } else {
            bounds
        })
    }
    pub(crate) fn path(path: &Path, surface: &Surface<'_>) -> Self {
        let b = path.bounds();
        Self {
            left: b.left().floor().clamp(0.0, surface.width as f32) as usize,
            top: b.top().floor().clamp(0.0, surface.height as f32) as usize,
            right: b.right().ceil().clamp(0.0, surface.width as f32) as usize,
            bottom: b.bottom().ceil().clamp(0.0, surface.height as f32) as usize,
        }
    }
    pub(crate) fn intersect(self, other: Self) -> Self {
        let result = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        if result.is_empty() {
            Self::EMPTY
        } else {
            result
        }
    }
    pub(crate) fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }
    /// The parts of `self` outside `inner`, which it contains: the rows above
    /// and below, then the columns left and right of it.
    pub(crate) fn around(self, inner: Self) -> [Self; 4] {
        [
            Self {
                bottom: inner.top,
                ..self
            },
            Self {
                top: inner.bottom,
                ..self
            },
            Self {
                top: inner.top,
                bottom: inner.bottom,
                right: inner.left,
                ..self
            },
            Self {
                top: inner.top,
                bottom: inner.bottom,
                left: inner.right,
                ..self
            },
        ]
    }
    pub(crate) fn rows(self, width: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
        (self.top..self.bottom).map(move |y| y * width + self.left..y * width + self.right)
    }
}
