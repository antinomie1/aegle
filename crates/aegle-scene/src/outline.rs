//! Outlines from SVG path data and circular arcs.

use crate::{Path, PathBuilder, Point};

impl PathBuilder {
    /// Starts a contour on the circle of `radius` around `center` at angle
    /// `start` and follows it for `sweep`, both in radians clockwise from the
    /// positive x axis (y points down); a negative sweep runs the other way.
    /// Each quarter turn is one cubic segment.
    pub fn arc(&mut self, center: Point, radius: f32, start: f32, sweep: f32) -> &mut Self {
        let at = |angle: f32| {
            let (sin, cos) = angle.sin_cos();
            Point::new(center.x + radius * cos, center.y + radius * sin)
        };
        self.move_to(at(start));
        let segments = (sweep.abs() / core::f32::consts::FRAC_PI_2).ceil().max(1.0);
        let step = sweep / segments;
        // Control points along the tangents: 4/3·tan(θ/4) of the radius.
        let reach = radius * 4.0 / 3.0 * (step / 4.0).tan();
        for index in 0..segments as u32 {
            let (from, to) = (
                start + step * index as f32,
                start + step * (index + 1) as f32,
            );
            let (a, b) = (at(from), at(to));
            let (sin_a, cos_a) = from.sin_cos();
            let (sin_b, cos_b) = to.sin_cos();
            self.cubic_to(
                Point::new(a.x - reach * sin_a, a.y + reach * cos_a),
                Point::new(b.x + reach * sin_b, b.y - reach * cos_b),
                b,
            );
        }
        self
    }
}

#[cfg(feature = "svg")]
impl Path {
    /// Parses SVG path data (`M 2 2 L 22 2 Z`, every command including arcs
    /// and relative ones) into a nonzero-filled path in the same units.
    pub fn from_svg(data: &str) -> Result<Self, crate::SceneError> {
        use svgtypes::SimplePathSegment as Segment;
        let mut builder = PathBuilder::new();
        let point = |x: f64, y: f64| {
            let point = Point::new(x as f32, y as f32);
            (point.x.is_finite() && point.y.is_finite())
                .then_some(point)
                .ok_or(crate::SceneError::NonFinite)
        };
        for segment in svgtypes::SimplifyingPathParser::from(data) {
            match segment.map_err(|_| crate::SceneError::InvalidPathData)? {
                Segment::MoveTo { x, y } => builder.move_to(point(x, y)?),
                Segment::LineTo { x, y } => builder.line_to(point(x, y)?),
                Segment::Quadratic { x1, y1, x, y } => {
                    builder.quad_to(point(x1, y1)?, point(x, y)?)
                }
                Segment::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => builder.cubic_to(point(x1, y1)?, point(x2, y2)?, point(x, y)?),
                Segment::ClosePath => builder.close(),
            };
        }
        builder.try_finish(crate::FillRule::NonZero)
    }
}
