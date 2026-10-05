use aegle_scene::{Affine, RoundedRect};
use tiny_skia::{Path, PathBuilder, Transform};

use crate::RenderError;

// Simple UI geometry stays well inside the fixed-point rasterizer's useful
// range. Reject larger device coordinates instead of silently losing a draw.
const MAX_COORDINATE: f32 = 1_048_576.0;

pub(crate) fn build(
    mut builder: PathBuilder,
    shape: RoundedRect,
    stroke: Option<f32>,
    transform: Affine,
) -> Result<Path, RenderError> {
    let rect = shape.rect();
    let x = rect.origin.x;
    let y = rect.origin.y;
    let w = rect.size.width;
    let h = rect.size.height;
    let r = shape.radius();
    if let Some(width) = stroke {
        let half = width * 0.5;
        // A centered border is a ring. EvenOdd filling removes the inner shape;
        // no general path stroker or temporary tessellation is needed.
        rounded(
            &mut builder,
            [x - half, y - half, w + width, h + width],
            if r == 0.0 { 0.0 } else { r + half },
        );
        if w > width && h > width {
            rounded(
                &mut builder,
                [x + half, y + half, w - width, h - width],
                (r - half).max(0.0),
            );
        }
    } else {
        rounded(&mut builder, [x, y, w, h], r);
    }
    let [a, b, c, d, e, f] = transform.coefficients();
    let path = builder
        .finish()
        .and_then(|path| path.transform(Transform::from_row(a, b, c, d, e, f)))
        .ok_or(RenderError::Coordinates)?;
    let b = path.bounds();
    validate_bounds([b.left(), b.top(), b.right(), b.bottom()])?;
    Ok(path)
}

// Inputs are finite: RoundedRect or tiny-skia's completed path validates them.
pub(crate) fn validate_bounds(bounds: [f32; 4]) -> Result<(), RenderError> {
    if bounds.iter().any(|v| v.abs() > MAX_COORDINATE) {
        return Err(RenderError::Coordinates);
    }
    Ok(())
}

fn rounded(pb: &mut PathBuilder, [x, y, w, h]: [f32; 4], r: f32) {
    let right = x + w;
    let bottom = y + h;
    let k = r * 0.552_284_8;
    pb.move_to(x + r, y);
    pb.line_to(right - r, y);
    if r > 0.0 {
        pb.cubic_to(right - r + k, y, right, y + r - k, right, y + r);
    }
    pb.line_to(right, bottom - r);
    if r > 0.0 {
        pb.cubic_to(
            right,
            bottom - r + k,
            right - r + k,
            bottom,
            right - r,
            bottom,
        );
    }
    pb.line_to(x + r, bottom);
    if r > 0.0 {
        pb.cubic_to(x + r - k, bottom, x, bottom - r + k, x, bottom - r);
    }
    pb.line_to(x, y + r);
    if r > 0.0 {
        pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    }
    pb.close();
}
