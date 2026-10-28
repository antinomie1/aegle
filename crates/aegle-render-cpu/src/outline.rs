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
    let r = shape.radius();
    match stroke {
        Some(width) => {
            let half = width * 0.5;
            rounded(
                [x - half, y - half, w + width, h + width],
                if r == 0.0 { 0.0 } else { r + half },
                &mut emit,
            );
            if w > width && h > width {
                rounded(
                    [x + half, y + half, w - width, h - width],
                    (r - half).max(0.0),
                    &mut emit,
                );
            }
        }
        None => rounded([x, y, w, h], r, &mut emit),
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

fn rounded([x, y, w, h]: [f32; 4], r: f32, emit: &mut impl FnMut(Segment)) {
    let right = x + w;
    let bottom = y + h;
    let k = r * 0.552_284_8;
    emit(Segment::Move { x: x + r, y });
    emit(Segment::Line { x: right - r, y });
    if r > 0.0 {
        emit(Segment::Cubic {
            x1: right - r + k,
            y1: y,
            x2: right,
            y2: y + r - k,
            x: right,
            y: y + r,
        });
    }
    emit(Segment::Line {
        x: right,
        y: bottom - r,
    });
    if r > 0.0 {
        emit(Segment::Cubic {
            x1: right,
            y1: bottom - r + k,
            x2: right - r + k,
            y2: bottom,
            x: right - r,
            y: bottom,
        });
    }
    emit(Segment::Line {
        x: x + r,
        y: bottom,
    });
    if r > 0.0 {
        emit(Segment::Cubic {
            x1: x + r - k,
            y1: bottom,
            x2: x,
            y2: bottom - r + k,
            x,
            y: bottom - r,
        });
    }
    emit(Segment::Line { x, y: y + r });
    if r > 0.0 {
        emit(Segment::Cubic {
            x1: x,
            y1: y + r - k,
            x2: x + r - k,
            y2: y,
            x: x + r,
            y,
        });
    }
    emit(Segment::Close);
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
