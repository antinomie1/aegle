//! Gradient fills and analytic shadows, sampled at pixel centers with the same
//! formulas as the GPU geometry shader.

use aegle_scene::{Affine, Gradient, GradientGeometry, RoundedRect};
use aegle_types::{Color, Point};
use tiny_skia::FillRule;

use crate::{
    Frame, RenderError,
    blend::{blend_linear, linear_rgba},
    path,
    raster::{Bounds, State, coverage_product},
};

impl Frame<'_, '_, '_> {
    pub(crate) fn paint_gradient(
        &mut self,
        shape: RoundedRect,
        gradient: &Gradient,
        state: State,
    ) -> Result<(), RenderError> {
        if shape.is_empty() || state.bounds.is_empty() {
            return Ok(());
        }
        let inverse = inverse(state.transform)?;
        let mut stops = [(0.0, [0.0; 4]); Gradient::MAX_STOPS];
        for (stop, entry) in gradient.stops().iter().zip(&mut stops) {
            *entry = (stop.offset, linear_rgba(stop.color.to_rgba()));
        }
        let stops = &stops[..gradient.stops().len()];
        let path = path::build(
            std::mem::take(&mut self.renderer.path),
            shape,
            None,
            state.transform,
        )?;
        let bounds = state.bounds.intersect(Bounds::path(&path, self.surface));
        let (coverage, clips) = self.renderer.masks.split_at_mut(1);
        crate::raster::rasterize(&mut coverage[0], &path, bounds, FillRule::EvenOdd);
        let (mask, clip) = (coverage[0].data(), state.clips.checked_sub(1));
        let clip = clip.map(|index| clips[index].data());
        let width = self.surface.width as usize;
        for (y, row) in (bounds.top..).zip(bounds.rows(width)) {
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            for ((x, i), pixel) in (bounds.left..).zip(row).zip(pixels.chunks_exact_mut(4)) {
                let alpha = clip.map_or(mask[i], |clip| coverage_product(mask[i], clip[i]));
                if alpha == 0 {
                    continue;
                }
                let local = inverse.map_point(Point::new(x as f32 + 0.5, y as f32 + 0.5));
                blend_linear(pixel, ramp(stops, offset(gradient, local)), alpha);
            }
        }
        self.renderer.path = path.clear();
        Ok(())
    }

    pub(crate) fn paint_shadow(
        &mut self,
        shape: RoundedRect,
        color: Color,
        blur: f32,
        state: State,
    ) -> Result<(), RenderError> {
        if shape.is_empty() || color.to_rgba()[3] == 0 || state.bounds.is_empty() {
            return Ok(());
        }
        let inverse = inverse(state.transform)?;
        let rect = shape.rect();
        let reach = blur * 3.0;
        let (x0, y0) = (rect.origin.x - reach, rect.origin.y - reach);
        let (x1, y1) = (
            rect.origin.x + rect.size.width + reach,
            rect.origin.y + rect.size.height + reach,
        );
        let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
            .map(|(x, y)| state.transform.map_point(Point::new(x, y)));
        let mut device = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for corner in corners {
            device = [
                device[0].min(corner.x),
                device[1].min(corner.y),
                device[2].max(corner.x),
                device[3].max(corner.y),
            ];
        }
        path::validate_bounds(device)?;
        let (width, height) = (self.surface.width as f32, self.surface.height as f32);
        let bounds = state.bounds.intersect(Bounds {
            left: device[0].floor().clamp(0.0, width) as usize,
            top: device[1].floor().clamp(0.0, height) as usize,
            right: device[2].ceil().clamp(0.0, width) as usize,
            bottom: device[3].ceil().clamp(0.0, height) as usize,
        });
        let paint = linear_rgba(color.to_rgba());
        let clip = state.clips.checked_sub(1);
        let clip = clip.map(|index| self.renderer.masks[index + 1].data());
        let half = (rect.size.width * 0.5, rect.size.height * 0.5);
        let center = (rect.origin.x + half.0, rect.origin.y + half.1);
        let shadow = Shadow {
            half,
            corner: shape.radius(),
            sigma: blur,
        };
        let row_width = self.surface.width as usize;
        for (y, row) in (bounds.top..).zip(bounds.rows(row_width)) {
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            for ((x, i), pixel) in (bounds.left..).zip(row).zip(pixels.chunks_exact_mut(4)) {
                let alpha = clip.map_or(255, |clip| clip[i]);
                let local = inverse.map_point(Point::new(x as f32 + 0.5, y as f32 + 0.5));
                let value = shadow.at(local.x - center.0, local.y - center.1);
                if alpha > 0 && value > 0.0 {
                    blend_linear(pixel, paint.map(|channel| channel * value), alpha);
                }
            }
        }
        Ok(())
    }
}

fn inverse(transform: Affine) -> Result<Affine, RenderError> {
    transform.inverse().map_err(|_| RenderError::Coordinates)
}

/// The position of `point` along `gradient`, before padding.
fn offset(gradient: &Gradient, point: Point) -> f32 {
    match gradient.geometry() {
        GradientGeometry::Linear { start, end } => {
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            ((point.x - start.x) * dx + (point.y - start.y) * dy) / (dx * dx + dy * dy)
        }
        GradientGeometry::Radial { center, radius } => {
            (point.x - center.x).hypot(point.y - center.y) / radius
        }
    }
}

/// Premultiplied linear color at offset `t`, padded with the end colors.
fn ramp(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    let (mut offset, mut color) = stops[0];
    if t <= offset {
        return color;
    }
    for &(next_offset, next) in &stops[1..] {
        if t < next_offset {
            let f = (t - offset) / (next_offset - offset);
            return std::array::from_fn(|i| color[i] + (next[i] - color[i]) * f);
        }
        (offset, color) = (next_offset, next);
    }
    color
}

/// A rounded rectangle convolved with a Gaussian, centered at the origin:
/// exact across x; across y, four rows weighted by the Gaussian's exact mass
/// over their intervals within four deviations (after Evan Wallace).
struct Shadow {
    half: (f32, f32),
    corner: f32,
    sigma: f32,
}

impl Shadow {
    fn at(&self, x: f32, y: f32) -> f32 {
        let (low, high) = (y - self.half.1, y + self.half.1);
        let start = (-4.0 * self.sigma).clamp(low, high);
        let end = (4.0 * self.sigma).clamp(low, high);
        let step = (end - start) * 0.25;
        let scale = std::f32::consts::FRAC_1_SQRT_2 / self.sigma;
        let mut value = 0.0;
        for index in 0..4 {
            let (from, to) = (
                start + step * index as f32,
                start + step * (index + 1) as f32,
            );
            let mass = 0.5 * (erf(to * scale) - erf(from * scale));
            value += self.row(x, y - (from + to) * 0.5) * mass;
        }
        value.clamp(0.0, 1.0)
    }

    fn row(&self, x: f32, y: f32) -> f32 {
        let delta = (self.half.1 - self.corner - y.abs()).min(0.0);
        let curved =
            self.half.0 - self.corner + (self.corner * self.corner - delta * delta).max(0.0).sqrt();
        let scale = std::f32::consts::FRAC_1_SQRT_2 / self.sigma;
        let integral = |edge: f32| 0.5 + 0.5 * erf(edge * scale);
        integral(x + curved) - integral(x - curved)
    }
}

/// Abramowitz–Stegun approximation, within 5e-4.
fn erf(x: f32) -> f32 {
    let a = x.abs();
    let r = 1.0 + (0.278_393 + (0.230_389 + 0.078_108 * a * a) * a) * a;
    let r = r * r;
    x.signum() * (1.0 - 1.0 / (r * r))
}
