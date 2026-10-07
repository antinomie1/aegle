//! A few rectangles covering a changed area, so that separate small changes
//! are not merged into one large bounding box.

use crate::{PixelRect, Rect};

/// A rectangle a [`Region`] can hold.
pub trait Area: Copy + Default {
    /// The smallest rectangle containing both.
    fn union(self, other: Self) -> Self;
    /// Whether the two share any point, edges included.
    fn touches(self, other: Self) -> bool;
    /// Covered area, for choosing the cheapest merge.
    fn area(self) -> f32;
    /// Whether nothing is covered.
    fn is_empty(self) -> bool;
}

impl Area for Rect {
    fn union(self, other: Self) -> Self {
        Rect::union(self, other)
    }
    fn touches(self, other: Self) -> bool {
        self.origin.x <= other.origin.x + other.size.width
            && other.origin.x <= self.origin.x + self.size.width
            && self.origin.y <= other.origin.y + other.size.height
            && other.origin.y <= self.origin.y + self.size.height
    }
    fn area(self) -> f32 {
        self.size.width * self.size.height
    }
    fn is_empty(self) -> bool {
        Rect::is_empty(self)
    }
}

impl Area for PixelRect {
    fn union(self, other: Self) -> Self {
        PixelRect::union(self, other)
    }
    fn touches(self, other: Self) -> bool {
        self.x <= other.x + other.width
            && other.x <= self.x + self.width
            && self.y <= other.y + other.height
            && other.y <= self.y + self.height
    }
    fn area(self) -> f32 {
        self.width as f32 * self.height as f32
    }
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// At most four rectangles covering everything added. Touching rectangles
/// merge; a fifth merges with the one whose union grows least.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Region<R> {
    rects: [R; 4],
    len: usize,
}

impl<R: Area> Region<R> {
    /// Adds `rect`, merging as described on the type.
    pub fn add(&mut self, rect: R) {
        if rect.is_empty() {
            return;
        }
        let mut rect = rect;
        loop {
            match self.rects[..self.len].iter().position(|r| r.touches(rect)) {
                Some(index) => rect = rect.union(self.remove(index)),
                None if self.len < self.rects.len() => break,
                None => {
                    let growth = |r: R| r.union(rect).area() - r.area() - rect.area();
                    let index = (1..self.len).fold(0, |best, i| {
                        if growth(self.rects[i]) < growth(self.rects[best]) {
                            i
                        } else {
                            best
                        }
                    });
                    rect = rect.union(self.remove(index));
                }
            }
        }
        self.rects[self.len] = rect;
        self.len += 1;
    }

    /// Adds every rectangle of `other`.
    pub fn extend(&mut self, other: &Self) {
        other.rects().iter().for_each(|&rect| self.add(rect));
    }

    /// The covering rectangles, disjoint and not touching.
    pub fn rects(&self) -> &[R] {
        &self.rects[..self.len]
    }

    /// Whether nothing was added.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn remove(&mut self, index: usize) -> R {
        let rect = self.rects[index];
        self.len -= 1;
        self.rects[index] = self.rects[self.len];
        rect
    }
}
