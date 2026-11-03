//! Shape and path outlines in a form every CPU rasterizer can consume.
//!
//! Both CPU backends must rasterize the same geometry, so the outline is built
//! here, once, and only the rasterizer's own path object differs. Segments are
//! emitted in local coordinates; the backend applies the device transform.

use aegle_scene::{Path, RoundedRect, Verb};

use crate::RenderError;

// Simple UI geometry stays well inside a rasterizer's useful range. Reject larger
// device coordinates instead of silently losing a draw.
const MAX_COORDINATE: f32 = 1_048_576.0;

/// One outline segment in local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// Starts a contour at the given point.
    Move {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
    },
    /// Straight segment to the given point.
    Line {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
    },
    /// Cubic segment through two control points.
    Cubic {
        /// First control point x.
        x1: f32,
        /// First control point y.
        y1: f32,
        /// Second control point x.
        x2: f32,
        /// Second control point y.
        y2: f32,
        /// End point x.
        x: f32,
        /// End point y.
        y: f32,
    },
    /// Closes the current contour.
    Close,
}

/// Emits the outline of a rounded rectangle, or of a centered border ring.
///
/// A border is two subpaths — the outside grown by half the width and the hole —
/// meant to be filled with the even-odd rule, as [`crate::Frame`] does. Zero
/// radius keeps square corners. Nothing is allocated.
pub fn shape_segments(shape: RoundedRect, stroke: Option<f32>, mut emit: impl FnMut(Segment)) {
    let rect = shape.rect();
    let (x, y) = (rect.origin.x, rect.origin.y);
    let (w, h) = (rect.size.width, rect.size.height);
    let radii = shape.radii();
    match stroke {
        Some(width) => {
            let half = width * 0.5;
            rounded(
                [x - half, y - half, w + width, h + width],
                radii.map(|r| if r == 0.0 { 0.0 } else { r + half }),
                &mut emit,
            );
            if w > width && h > width {
                rounded(
                    [x + half, y + half, w - width, h - width],
                    radii.map(|r| (r - half).max(0.0)),
                    &mut emit,
                );
            }
        }
        None => rounded([x, y, w, h], radii, &mut emit),
    }
}

/// Emits the segments of a validated scene path.
pub fn path_segments(path: &Path, mut emit: impl FnMut(Segment)) {
    let mut points = path.points().iter();
    let mut next = || points.next().expect("verbs and points agree");
    for verb in path.verbs() {
        match verb {
            Verb::Move => {
                let point = next();
                emit(Segment::Move {
                    x: point.x,
                    y: point.y,
                });
            }
            Verb::Line => {
                let point = next();
                emit(Segment::Line {
                    x: point.x,
                    y: point.y,
                });
            }
            Verb::Quad => {
                let (point, end) = (next(), next());
                // Quadratics are promoted to cubics so every backend receives one
                // segment kind, using the exact elevation.
                emit(Segment::Cubic {
                    x1: point.x + 2.0 / 3.0 * (end.x - point.x),
                    y1: point.y + 2.0 / 3.0 * (end.y - point.y),
                    x2: end.x + 2.0 / 3.0 * (point.x - end.x),
                    y2: end.y + 2.0 / 3.0 * (point.y - end.y),
                    x: end.x,
                    y: end.y,
                });
            }
            Verb::Cubic => {
                let (first, second, end) = (next(), next(), next());
                emit(Segment::Cubic {
                    x1: first.x,
                    y1: first.y,
                    x2: second.x,
                    y2: second.y,
                    x: end.x,
                    y: end.y,
                });
            }
            Verb::Close => emit(Segment::Close),
        }
    }
}

/// A rounded rectangle with top-left, top-right, bottom-right and
/// bottom-left radii, clockwise from the end of the top-left corner.
fn rounded(
    [x, y, w, h]: [f32; 4],
    [top_left, top_right, bottom_right, bottom_left]: [f32; 4],
    emit: &mut impl FnMut(Segment),
) {
    let (right, bottom) = (x + w, y + h);
    emit(Segment::Move { x: x + top_left, y });
    emit(Segment::Line {
        x: right - top_right,
        y,
    });
    corner(
        emit,
        (right - top_right, y),
        (right, y),
        (right, y + top_right),
    );
    emit(Segment::Line {
        x: right,
        y: bottom - bottom_right,
    });
    corner(
        emit,
        (right, bottom - bottom_right),
        (right, bottom),
        (right - bottom_right, bottom),
    );
    emit(Segment::Line {
        x: x + bottom_left,
        y: bottom,
    });
    corner(
        emit,
        (x + bottom_left, bottom),
        (x, bottom),
        (x, bottom - bottom_left),
    );
    emit(Segment::Line { x, y: y + top_left });
    corner(emit, (x, y + top_left), (x, y), (x + top_left, y));
    emit(Segment::Close);
}

/// A quarter circle from `from` to `to` around the square corner `apex`, its
/// control points 0.5523 of the radius from the ends toward the apex;
/// nothing for a square corner.
fn corner(emit: &mut impl FnMut(Segment), from: (f32, f32), apex: (f32, f32), to: (f32, f32)) {
    if from == to {
        return;
    }
    let k = 0.552_284_8;
    emit(Segment::Cubic {
        x1: from.0 + (apex.0 - from.0) * k,
        y1: from.1 + (apex.1 - from.1) * k,
        x2: to.0 + (apex.0 - to.0) * k,
        y2: to.1 + (apex.1 - to.1) * k,
        x: to.0,
        y: to.1,
    });
}

/// Rejects device geometry a rasterizer cannot represent.
pub fn validate_bounds(bounds: [f32; 4]) -> Result<(), RenderError> {
    if bounds.iter().any(|value| value.abs() > MAX_COORDINATE) {
        return Err(RenderError::Coordinates);
    }
    Ok(())
}

/// The bounding box of `segments` mapped by `transform`, as
/// `[left, top, right, bottom]`.
pub fn device_bounds(
    segments: impl FnOnce(&mut dyn FnMut(Segment)),
    transform: aegle_scene::Affine,
) -> [f32; 4] {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    segments(&mut |segment| {
        let points: &[(f32, f32)] = match segment {
            Segment::Move { x, y } | Segment::Line { x, y } => &[(x, y)],
            Segment::Cubic {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => &[(x1, y1), (x2, y2), (x, y)],
            Segment::Close => &[],
        };
        for &(x, y) in points {
            let mapped = transform.map_point(aegle_scene::Point::new(x, y));
            bounds = [
                bounds[0].min(mapped.x),
                bounds[1].min(mapped.y),
                bounds[2].max(mapped.x),
                bounds[3].max(mapped.y),
            ];
        }
    });
    bounds
}
