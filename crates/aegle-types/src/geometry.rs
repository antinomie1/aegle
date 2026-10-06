/// A location in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

impl Point {
    /// Creates a point.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// An extent in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Horizontal extent.
    pub width: f32,
    /// Vertical extent.
    pub height: f32,
}

impl Size {
    /// Creates an extent.
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned, half-open rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Top-left corner.
    pub origin: Point,
    /// Width and height; nonpositive extents are empty.
    pub size: Size,
}

impl Rect {
    /// Creates a rectangle.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    /// Whether this rectangle has no positive area.
    pub fn is_empty(self) -> bool {
        self.size.width <= 0.0 || self.size.height <= 0.0
    }

    /// Tests a point, excluding the right and bottom edges.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x < self.origin.x + self.size.width
            && point.y < self.origin.y + self.size.height
    }

    /// Intersects this rectangle with another, returning no zero-area result.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.origin.x.max(other.origin.x);
        let y = self.origin.y.max(other.origin.y);
        let right = (self.origin.x + self.size.width).min(other.origin.x + other.size.width);
        let bottom = (self.origin.y + self.size.height).min(other.origin.y + other.size.height);
        (right > x && bottom > y).then(|| Self::new(x, y, right - x, bottom - y))
    }

    /// The smallest rectangle containing both.
    pub fn union(self, other: Self) -> Self {
        let x = self.origin.x.min(other.origin.x);
        let y = self.origin.y.min(other.origin.y);
        let right = (self.origin.x + self.size.width).max(other.origin.x + other.size.width);
        let bottom = (self.origin.y + self.size.height).max(other.origin.y + other.size.height);
        Self::new(x, y, right - x, bottom - y)
    }
}

/// An axis-aligned rectangle of whole buffer pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelRect {
    /// Left column.
    pub x: u32,
    /// Top row.
    pub y: u32,
    /// Columns covered.
    pub width: u32,
    /// Rows covered.
    pub height: u32,
}

impl PixelRect {
    /// The whole buffer of `width` × `height` pixels.
    pub const fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// The pixels touched by a logical rectangle at `scale`, rounded outward and
    /// limited to a `width` × `height` buffer.
    pub fn covering(rect: Rect, scale: f32, width: u32, height: u32) -> Self {
        // Clamped values are nonnegative, so truncation rounds down (no_std
        // has no floor or ceil).
        let edge = |value: f32, limit: u32| (value * scale).clamp(0.0, limit as f32);
        let up = |value: f32| value as u32 + u32::from((value as u32 as f32) < value);
        let left = edge(rect.origin.x, width) as u32;
        let top = edge(rect.origin.y, height) as u32;
        let right = up(edge(rect.origin.x + rect.size.width, width));
        let bottom = up(edge(rect.origin.y + rect.size.height, height));
        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }

    /// The smallest rectangle containing both.
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}
