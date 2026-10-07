//! Rounded-rectangle fills and borders. Without rotation or skew, a
//! sharp-cornered shape's coverage is exact area: each pixel's overlap with
//! the outer rectangle, less its overlap with a border's hole. No mask is
//! rasterized, the fully covered inside of a fill is written directly and the
//! hole of a border is skipped. A rounded border rasterizes its ring once and
//! reads only the bands around the hole; other shapes use the general path.

use aegle_scene::{Affine, RoundedRect};
use aegle_types::Color;
use tiny_skia::FillRule;

use crate::{
    RenderError,
    blend::Solid,
    path,
    raster::{Bounds, Frame, State, coverage_product},
};

/// Device edges `[left, top, right, bottom]`.
type Edges = [f32; 4];

impl Frame<'_, '_, '_> {
    pub(crate) fn paint(
        &mut self,
        shape: RoundedRect,
        color: Color,
        width: Option<f32>,
        state: State,
    ) -> Result<(), RenderError> {
        if shape.is_empty() || color.to_rgba()[3] == 0 || state.bounds.is_empty() {
            return Ok(());
        }
        if let [a, 0.0, 0.0, d, e, f] = state.transform.coefficients()
            && shape.radius() == 0.0
        {
            let (outer, hole) = sharp(shape, width, [a, d, e, f]);
            path::validate_bounds(outer)?;
            self.sharp(outer, hole, color, state);
            return Ok(());
        }
        let path = self.geometry(shape, width, state.transform)?;
        let bounds = state.bounds.intersect(Bounds::path(&path, self.surface));
        match rounded_hole(shape, width, state.transform).map(|hole| hole.intersect(bounds)) {
            // The ring lies in the bands: rasterize it once, read only them.
            Some(hole) if !hole.is_empty() => {
                let bands = bounds.around(hole);
                self.cover_parts(&path, FillRule::EvenOdd, color, state, &bands);
                self.renderer.path = path.clear();
            }
            _ => self.fill_device(path, FillRule::EvenOdd, color, state),
        }
        Ok(())
    }

    /// Draws an upright sharp-cornered fill, or a border when `hole` is set.
    fn sharp(&mut self, outer: Edges, hole: Option<Edges>, color: Color, state: State) {
        let (width, height) = (self.surface.width as f32, self.surface.height as f32);
        type Round = fn(f32) -> f32;
        let pixels = |[left, top, right, bottom]: Edges, outward: bool| {
            let (low, high): (Round, Round) = match outward {
                true => (f32::floor, f32::ceil),
                false => (f32::ceil, f32::floor),
            };
            Bounds {
                left: low(left).clamp(0.0, width) as usize,
                top: low(top).clamp(0.0, height) as usize,
                right: high(right).clamp(0.0, width) as usize,
                bottom: high(bottom).clamp(0.0, height) as usize,
            }
        };
        let bounds = state.bounds.intersect(pixels(outer, true));
        if bounds.is_empty() {
            return;
        }
        // Whole pixels of known coverage: none inside the hole, all inside a fill.
        let inside = pixels(hole.unwrap_or(outer), false).intersect(bounds);
        let parts = match inside.is_empty() {
            true => [bounds, Bounds::EMPTY, Bounds::EMPTY, Bounds::EMPTY],
            false => bounds.around(inside),
        };
        let clip = state
            .clips
            .checked_sub(1)
            .map(|index| self.renderer.masks[index + 1].data());
        let paint = Solid::new(self.surface.color(color));
        let stride = self.surface.width as usize;
        for row in parts.iter().flat_map(|part| part.rows(stride)) {
            let y = (row.start / stride) as f32;
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            paint.blend_span(pixels, row.start, |i| {
                let x = (i % stride) as f32;
                let area = overlap(outer, x, y) - hole.map_or(0.0, |hole| overlap(hole, x, y));
                let coverage = (area * 255.0 + 0.5) as u8;
                clip.map_or(coverage, |c| coverage_product(coverage, c[i]))
            });
        }
        if hole.is_none() && !inside.is_empty() {
            self.solid(inside, color, state);
        }
    }

    /// Fills whole pixels at full coverage, within the current clip.
    fn solid(&mut self, inner: Bounds, color: Color, state: State) {
        let clip = state
            .clips
            .checked_sub(1)
            .map(|index| self.renderer.masks[index + 1].data());
        let color = self.surface.color(color);
        let rgba = color.to_rgba();
        if clip.is_none() && rgba[3] == 255 {
            for row in inner.rows(self.surface.width as usize) {
                for pixel in self.surface.data[row.start * 4..row.end * 4].chunks_exact_mut(4) {
                    pixel.copy_from_slice(&rgba);
                }
            }
            return;
        }
        let paint = Solid::new(color);
        for row in inner.rows(self.surface.width as usize) {
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            paint.blend_span(pixels, row.start, |i| clip.map_or(255, |clip| clip[i]));
        }
    }
}

/// The area of pixel `(x, y)` inside the rectangle `edges`.
fn overlap([left, top, right, bottom]: Edges, x: f32, y: f32) -> f32 {
    let across = (right.min(x + 1.0) - left.max(x)).max(0.0);
    let down = (bottom.min(y + 1.0) - top.max(y)).max(0.0);
    across * down
}

/// The device edges of an upright sharp-cornered fill, or of a centered
/// border's outside and hole, as `path::build` outlines them.
fn sharp(shape: RoundedRect, width: Option<f32>, [a, d, e, f]: [f32; 4]) -> (Edges, Option<Edges>) {
    let rect = shape.rect();
    let (x, y) = (rect.origin.x, rect.origin.y);
    let (w, h) = (rect.size.width, rect.size.height);
    let map = |[x0, y0, x1, y1]: Edges| {
        let (left, right) = (a * x0 + e, a * x1 + e);
        let (top, bottom) = (d * y0 + f, d * y1 + f);
        [
            left.min(right),
            top.min(bottom),
            left.max(right),
            top.max(bottom),
        ]
    };
    let Some(width) = width else {
        return (map([x, y, x + w, y + h]), None);
    };
    let half = width * 0.5;
    let outer = map([x - half, y - half, x + w + half, y + h + half]);
    let hole =
        (w > width && h > width).then(|| map([x + half, y + half, x + w - half, y + h - half]));
    (outer, hole)
}

/// Whole device pixels inside an upright rounded border's hole beyond its
/// inner corners, with a one-pixel margin for antialiasing; `None` for fills.
fn rounded_hole(shape: RoundedRect, width: Option<f32>, transform: Affine) -> Option<Bounds> {
    let [a, b, c, d, e, f] = transform.coefficients();
    let rect = shape.rect();
    let (w, h) = (rect.size.width, rect.size.height);
    let width = width.filter(|&width| b == 0.0 && c == 0.0 && w > width && h > width)?;
    let half = width * 0.5;
    let inset = half + (shape.radius() - half).max(0.0);
    let (x0, x1) = (rect.origin.x + inset, rect.origin.x + w - inset);
    let (y0, y1) = (rect.origin.y + inset, rect.origin.y + h - inset);
    let (left, right) = ((a * x0 + e).min(a * x1 + e), (a * x0 + e).max(a * x1 + e));
    let (top, bottom) = ((d * y0 + f).min(d * y1 + f), (d * y0 + f).max(d * y1 + f));
    let edge = |value: f32| value.max(0.0) as usize;
    let hole = Bounds {
        left: edge(left.ceil() + 1.0),
        top: edge(top.ceil() + 1.0),
        right: edge(right.floor() - 1.0),
        bottom: edge(bottom.floor() - 1.0),
    };
    (!hole.is_empty()).then_some(hole)
}
