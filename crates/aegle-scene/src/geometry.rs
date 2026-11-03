use crate::{Point, Rect, SceneError};

/// A finite rectangle with a nonnegative radius for each corner.
///
/// Zero radii give square corners. Zero-sized shapes have no drawable area,
/// including when stroked; when used as clips they exclude all drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    rect: Rect,
    radii: [f32; 4],
}

impl RoundedRect {
    /// A rectangle with the same `radius` at every corner, clamped to half
    /// the smaller extent.
    ///
    /// # Panics
    ///
    /// On negative sizes or radius, nonfinite values and overflowing far edges.
    #[track_caller]
    pub fn new(rect: Rect, radius: f32) -> Self {
        Self::with_radii(rect, [radius; 4])
    }

    /// A rectangle with `radii` for its top-left, top-right, bottom-right and
    /// bottom-left corners. Where two corners on one side would overlap, all
    /// radii shrink by the same factor until they meet, as in CSS.
    ///
    /// # Panics
    ///
    /// On negative sizes or radii, nonfinite values and overflowing far edges.
    #[track_caller]
    pub fn with_radii(rect: Rect, radii: [f32; 4]) -> Self {
        crate::valid(Self::checked(rect, radii))
    }

    pub(crate) fn checked(rect: Rect, radii: [f32; 4]) -> Result<Self, SceneError> {
        let extents = [rect.size.width, rect.size.height];
        let values = [rect.origin.x, rect.origin.y];
        if !values
            .iter()
            .chain(&extents)
            .chain(&radii)
            .all(|v| v.is_finite())
        {
            return Err(SceneError::NonFinite);
        }
        if extents.iter().chain(&radii).any(|v| *v < 0.0) {
            return Err(SceneError::NegativeExtent);
        }
        if !(rect.origin.x + rect.size.width).is_finite()
            || !(rect.origin.y + rect.size.height).is_finite()
        {
            return Err(SceneError::CoordinateRange);
        }
        let [top_left, top_right, bottom_right, bottom_left] = radii;
        let sides = [
            (rect.size.width, top_left + top_right),
            (rect.size.width, bottom_left + bottom_right),
            (rect.size.height, top_left + bottom_left),
            (rect.size.height, top_right + bottom_right),
        ];
        let factor = sides
            .iter()
            .filter(|(_, sum)| *sum > 0.0)
            .fold(1.0f32, |factor, (length, sum)| factor.min(length / sum));
        Ok(Self {
            rect,
            radii: radii.map(|radius| radius * factor),
        })
    }

    /// Returns the rectangular bounds in logical pixels.
    pub const fn rect(self) -> Rect {
        self.rect
    }

    /// Returns the clamped top-left, top-right, bottom-right and bottom-left
    /// radii in logical pixels.
    pub const fn radii(self) -> [f32; 4] {
        self.radii
    }

    /// Whether every corner is square.
    pub fn is_square(self) -> bool {
        self.radii == [0.0; 4]
    }

    /// Whether this shape has zero width or height.
    pub fn is_empty(self) -> bool {
        self.rect.is_empty()
    }
}

/// A finite invertible two-dimensional affine transform.
///
/// Coefficients `[a, b, c, d, e, f]` map points as
/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f`. All coefficients of the
/// mathematical inverse must also be representable as finite `f32` values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine([f32; 6]);

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    /// Leaves coordinates unchanged.
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Validates coefficients and the finite, invertible transform they define.
    pub fn new(coefficients: [f32; 6]) -> Result<Self, SceneError> {
        if !coefficients.iter().all(|v| v.is_finite()) {
            return Err(SceneError::NonFinite);
        }
        let [a, b, c, d, e, f] = coefficients.map(f64::from);
        let determinant = a * d - b * c;
        if determinant == 0.0 {
            return Err(SceneError::InvalidTransform);
        }
        let inverse = [d, -b, -c, a, c * f - d * e, b * e - a * f];
        if !inverse
            .iter()
            .all(|v| ((v / determinant) as f32).is_finite())
        {
            return Err(SceneError::InvalidTransform);
        }
        Ok(Self(coefficients))
    }

    /// Creates a translation in logical pixels; panics unless both are finite.
    #[track_caller]
    pub fn translation(x: f32, y: f32) -> Self {
        crate::valid(Self::new([1.0, 0.0, 0.0, 1.0, x, y]))
    }

    /// Creates a scale; negative factors reflect. Panics on zero or nonfinite
    /// factors.
    #[track_caller]
    pub fn scale(x: f32, y: f32) -> Self {
        crate::valid(Self::new([x, 0.0, 0.0, y, 0.0, 0.0]))
    }

    /// Returns coefficients in `[a, b, c, d, e, f]` order.
    pub const fn coefficients(self) -> [f32; 6] {
        self.0
    }

    /// Maps a point using `f32` arithmetic, without validating the input or output.
    ///
    /// Scene recording checks mapped shape bounds; callers mapping arbitrary
    /// points are responsible for their own coordinate-range requirements.
    pub fn map_point(self, point: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * point.x + c * point.y + e, b * point.x + d * point.y + f)
    }

    /// Applies `self`, then `next`: the result is the matrix product `next * self`.
    ///
    /// Fails if composition overflows or the result has no finite inverse.
    pub fn then(self, next: Self) -> Result<Self, SceneError> {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = next.0;
        Self::new([
            g * a + i * b,
            h * a + j * b,
            g * c + i * d,
            h * c + j * d,
            g * e + i * f + k,
            h * e + j * f + l,
        ])
        .map_err(|_| SceneError::CoordinateRange)
    }

    /// Inverts the transform, rejecting a result made singular by `f32` rounding.
    pub fn inverse(self) -> Result<Self, SceneError> {
        let [a, b, c, d, e, f] = self.0.map(f64::from);
        let determinant = a * d - b * c;
        Self::new(
            [d, -b, -c, a, c * f - d * e, b * e - a * f].map(|value| (value / determinant) as f32),
        )
    }

    /// The bounding box of `rect`, grown by `outset` on every side, mapped
    /// through this transform.
    pub(crate) fn bounds(self, rect: Rect, outset: f32) -> Rect {
        let (x0, y0) = (rect.origin.x - outset, rect.origin.y - outset);
        let x1 = rect.origin.x + rect.size.width + outset;
        let y1 = rect.origin.y + rect.size.height + outset;
        let corners =
            [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| self.map_point(Point::new(x, y)));
        let (mut low, mut high) = (corners[0], corners[0]);
        for corner in corners {
            (low.x, low.y) = (low.x.min(corner.x), low.y.min(corner.y));
            (high.x, high.y) = (high.x.max(corner.x), high.y.max(corner.y));
        }
        Rect::new(low.x, low.y, high.x - low.x, high.y - low.y)
    }

    pub(crate) fn validate_shape(self, shape: RoundedRect, outset: f32) -> Result<(), SceneError> {
        let rect = shape.rect;
        let x0 = rect.origin.x - outset;
        let y0 = rect.origin.y - outset;
        let x1 = rect.origin.x + rect.size.width + outset;
        let y1 = rect.origin.y + rect.size.height + outset;
        for point in [
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x0, y1),
            Point::new(x1, y1),
        ] {
            let mapped = self.map_point(point);
            if !mapped.x.is_finite() || !mapped.y.is_finite() {
                return Err(SceneError::CoordinateRange);
            }
        }
        Ok(())
    }
}
