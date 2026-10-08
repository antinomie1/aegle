use crate::{Color, Point};

/// A soft shadow beneath a shape, following its corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Finite logical displacement of the shadow from the shape.
    pub offset: Point,
    /// Nonnegative Gaussian standard deviation in logical pixels; zero is a
    /// hard-edged copy of the shape.
    pub blur: f32,
    /// Finite logical growth (or, when negative, shrinkage) of the shape.
    pub spread: f32,
    /// Unpremultiplied sRGB color.
    pub color: Color,
}

impl Shadow {
    /// Whether the geometry is finite and the blur nonnegative.
    pub fn is_valid(self) -> bool {
        let finite = [self.offset.x, self.offset.y, self.blur, self.spread].map(f32::is_finite);
        !finite.contains(&false) && self.blur >= 0.0
    }

    /// Whether the color is fully transparent, so nothing is drawn.
    pub fn is_clear(self) -> bool {
        self.color.to_rgba()[3] == 0
    }
}
