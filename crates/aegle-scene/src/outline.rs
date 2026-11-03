//! Outlines from SVG path data.

use crate::{PathBuilder, Point};

impl crate::Path {
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
