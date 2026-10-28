//! Arbitrary path coverage through tiny-skia and bilinear image sampling.

use aegle_scene::{Affine, FillRule, Image, LineCap, LineJoin, Path, Rect, Stroke, Verb};
use aegle_types::{Color, Point};
use tiny_skia::{PathStroker, Transform};

use crate::{
    Frame, RenderError, Surface,
    blend::{blend_linear_span, linear_rgba},
    path,
    raster::{Bounds, State},
    surface::order,
};

impl Frame<'_, '_, '_> {
    pub(crate) fn paint_path(
        &mut self,
        path: &Path,
        color: Color,
        stroke: Option<Stroke>,
        state: State,
    ) -> Result<(), RenderError> {
        if color.to_rgba()[3] == 0 || state.bounds.is_empty() {
            return Ok(());
        }
        let mut builder = std::mem::take(&mut self.renderer.path);
        let mut points = path.points().iter().map(|p| (p.x, p.y));
        let mut next = || points.next().unwrap();
        for verb in path.verbs() {
            match verb {
                Verb::Move => {
                    let (x, y) = next();
                    builder.move_to(x, y);
                }
                Verb::Line => {
                    let (x, y) = next();
                    builder.line_to(x, y);
                }
                Verb::Quad => {
                    let ((x1, y1), (x, y)) = (next(), next());
                    builder.quad_to(x1, y1, x, y);
                }
                Verb::Cubic => {
                    let ((x1, y1), (x2, y2), (x, y)) = (next(), next(), next());
                    builder.cubic_to(x1, y1, x2, y2, x, y);
                }
                Verb::Close => builder.close(),
            }
        }
        // Fewer than two points or degenerate strokes cover no pixel.
        let Some(local) = builder.finish() else {
            return Ok(());
        };
        let [a, b, c, d, e, f] = state.transform.coefficients();
        let transform = Transform::from_row(a, b, c, d, e, f);
        let (local, rule) = match stroke {
            None => (local, path.fill_rule()),
            Some(stroke) => {
                let style = tiny_skia::Stroke {
                    width: stroke.width,
                    miter_limit: 4.0,
                    line_cap: match stroke.cap {
                        LineCap::Butt => tiny_skia::LineCap::Butt,
                        LineCap::Round => tiny_skia::LineCap::Round,
                        LineCap::Square => tiny_skia::LineCap::Square,
                    },
                    line_join: match stroke.join {
                        LineJoin::Miter => tiny_skia::LineJoin::Miter,
                        LineJoin::Round => tiny_skia::LineJoin::Round,
                        LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                    },
                    dash: None,
                };
                let scale = PathStroker::compute_resolution_scale(&transform);
                match local.stroke(&style, scale) {
                    Some(outline) => (outline, FillRule::NonZero),
                    None => return Ok(()),
                }
            }
        };
        let device = local.transform(transform).ok_or(RenderError::Coordinates)?;
        let b = device.bounds();
        path::validate_bounds([b.left(), b.top(), b.right(), b.bottom()])?;
        let rule = match rule {
            FillRule::NonZero => tiny_skia::FillRule::Winding,
            FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        };
        self.fill_device(device, rule, color, state);
        Ok(())
    }

    pub(crate) fn paint_image(
        &mut self,
        image: &Image,
        rect: Rect,
        state: State,
    ) -> Result<(), RenderError> {
        if state.bounds.is_empty() {
            return Ok(());
        }
        let (width, height) = (image.width(), image.height());
        let transform = Affine::new([
            rect.size.width / width as f32,
            0.0,
            0.0,
            rect.size.height / height as f32,
            rect.origin.x,
            rect.origin.y,
        ])
        .and_then(|local| local.then(state.transform))
        .map_err(|_| RenderError::Coordinates)?;
        let [a, b, c, d, e, f] = transform.coefficients();
        // Unscaled, pixel-aligned images copy texels without filtering.
        let aligned = [a, b, c, d] == [1.0, 0.0, 0.0, 1.0] && e.fract() == 0.0 && f.fract() == 0.0;
        let bounds = device_bounds(width, height, transform, self.surface, !aligned)?
            .intersect(state.bounds);
        let inverse = transform.inverse().map_err(|_| RenderError::Coordinates)?;
        // Local extent of one device pixel, as `fwidth` in the GPU shader.
        let [ia, ib, ic, id, _, _] = inverse.coefficients();
        let footprint = [
            (ia.abs() + ic.abs()).max(1e-6),
            (ib.abs() + id.abs()).max(1e-6),
        ];
        let edge = |p: f32, size: f32, footprint: f32| {
            ((size - p) / footprint + 0.5).clamp(0.0, 1.0) - (0.5 - p / footprint).clamp(0.0, 1.0)
        };
        let clip = (state.clips > 0).then(|| self.renderer.masks[state.clips].data());
        let (stride, bgra) = (self.surface.width as usize, self.surface.bgra);
        for row in bounds.rows(stride) {
            let y = (row.start / stride) as f32 + 0.5;
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            blend_linear_span(pixels, row.start, |i, pixel| {
                let local = inverse.map_point(Point::new((i % stride) as f32 + 0.5, y));
                let coverage = clip.map_or(255, |mask| mask[i]);
                if aligned {
                    let texel = order(bgra, fetch(image, local.x as u32, local.y as u32));
                    if texel[3] == 255 && coverage == 255 {
                        pixel.copy_from_slice(&texel);
                        return ([0.0; 4], 0);
                    }
                    return (linear_rgba(texel), coverage);
                }
                let alpha = edge(local.x, width as f32, footprint[0])
                    * edge(local.y, height as f32, footprint[1]);
                let source =
                    order(bgra, sample(image, local.x - 0.5, local.y - 0.5)).map(|v| v * alpha);
                (source, coverage)
            });
        }
        Ok(())
    }
}

/// Device pixels touched by a `width`×`height` image under `transform`,
/// including half a pixel of bilinear support when `filtered`.
pub(crate) fn device_bounds(
    width: u32,
    height: u32,
    transform: Affine,
    surface: &Surface<'_>,
    filtered: bool,
) -> Result<Bounds, RenderError> {
    let (w, h) = (width as f32, height as f32);
    let outset = if filtered { 0.5 } else { 0.0 };
    let corners = [
        Point::new(-outset, -outset),
        Point::new(w + outset, -outset),
        Point::new(-outset, h + outset),
        Point::new(w + outset, h + outset),
    ]
    .map(|point| transform.map_point(point));
    if corners.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(RenderError::Coordinates);
    }
    let fold = |pick: fn(&Point) -> f32, init: f32, f: fn(f32, f32) -> f32| {
        corners.iter().map(pick).fold(init, f)
    };
    let x0 = fold(|p| p.x, f32::INFINITY, f32::min);
    let y0 = fold(|p| p.y, f32::INFINITY, f32::min);
    let x1 = fold(|p| p.x, f32::NEG_INFINITY, f32::max);
    let y1 = fold(|p| p.y, f32::NEG_INFINITY, f32::max);
    path::validate_bounds([x0, y0, x1, y1])?;
    Ok(Bounds {
        left: x0.floor().clamp(0.0, surface.width as f32) as usize,
        top: y0.floor().clamp(0.0, surface.height as f32) as usize,
        right: x1.ceil().clamp(0.0, surface.width as f32) as usize,
        bottom: y1.ceil().clamp(0.0, surface.height as f32) as usize,
    })
}

fn fetch(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width() as usize + x as usize) * 4;
    image.pixels()[index..index + 4].try_into().unwrap()
}

/// Bilinear premultiplied linear sample at texel-center coordinates, clamped
/// to the edge texels; the caller applies analytic edge coverage.
fn sample(image: &Image, x: f32, y: f32) -> [f32; 4] {
    let max = [image.width() - 1, image.height() - 1];
    let (x, y) = (x.clamp(0.0, max[0] as f32), y.clamp(0.0, max[1] as f32));
    let (left, top) = (x.floor(), y.floor());
    let (dx, dy) = (x - left, y - top);
    let (x0, y0) = (left as u32, top as u32);
    let (x1, y1) = ((x0 + 1).min(max[0]), (y0 + 1).min(max[1]));
    let mut result = [0.0; 4];
    for (tx, ty, weight) in [
        (x0, y0, (1.0 - dx) * (1.0 - dy)),
        (x1, y0, dx * (1.0 - dy)),
        (x0, y1, (1.0 - dx) * dy),
        (x1, y1, dx * dy),
    ] {
        let texel = linear_rgba(fetch(image, tx, ty));
        for (channel, value) in result.iter_mut().zip(texel) {
            *channel += value * weight;
        }
    }
    result
}
