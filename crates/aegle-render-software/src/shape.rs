//! Rounded-rectangle fills and borders. Without rotation or skew, the inside
//! of a sharp-cornered fill is fully covered and the hole of a border is not
//! covered at all, so only the bands around that interior need a coverage
//! mask; the interior is written directly or skipped.

use aegle_scene::{Affine, RoundedRect};
use aegle_types::Color;
use tiny_skia::{FillRule, PathBuilder, Rect};

use crate::{
    RenderError,
    blend::Solid,
    raster::{Bounds, Frame, State},
};

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
        let path = self.geometry(shape, width, state.transform)?;
        let bounds = state.bounds.intersect(Bounds::path(&path, self.surface));
        let inner = interior(shape, width, state.transform)
            .map(|inner| inner.intersect(bounds))
            .filter(|inner| !inner.is_empty());
        let Some(inner) = inner else {
            self.fill_device(path, FillRule::EvenOdd, color, state);
            return Ok(());
        };
        let outline = path.bounds();
        for band in bounds.around(inner) {
            if band.is_empty() {
                continue;
            }
            if width.is_some() {
                self.cover(&path, FillRule::EvenOdd, color, state, band);
                continue;
            }
            // Cut at whole pixels, the rectangle covers the band's pixels as
            // the whole one does, without rasterizing its interior.
            let cut = Rect::from_ltrb(
                outline.left().max(band.left as f32),
                outline.top().max(band.top as f32),
                outline.right().min(band.right as f32),
                outline.bottom().min(band.bottom as f32),
            );
            if let Some(cut) = cut {
                self.cover(
                    &PathBuilder::from_rect(cut),
                    FillRule::EvenOdd,
                    color,
                    state,
                    band,
                );
            }
        }
        if width.is_none() {
            self.solid(inner, color, state);
        }
        self.renderer.path = path.clear();
        Ok(())
    }

    /// Fills whole pixels at full coverage, within the current clip.
    fn solid(&mut self, inner: Bounds, color: Color, state: State) {
        let clip = state
            .clips
            .checked_sub(1)
            .map(|index| self.renderer.masks[index + 1].data());
        let paint = Solid::new(color);
        for row in inner.rows(self.surface.width as usize) {
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            for (i, pixel) in row.zip(pixels.chunks_exact_mut(4)) {
                paint.blend(pixel, clip.map_or(255, |clip| clip[i]));
            }
        }
    }
}

/// Whole device pixels of known coverage for an upright shape: fully inside
/// a sharp-cornered fill, or inside a border's hole beyond its inner corners.
/// A one-pixel margin keeps every antialiased edge in the bands.
fn interior(shape: RoundedRect, width: Option<f32>, transform: Affine) -> Option<Bounds> {
    let [a, b, c, d, e, f] = transform.coefficients();
    if b != 0.0 || c != 0.0 {
        return None;
    }
    let rect = shape.rect();
    let inset = match width {
        None if shape.radius() == 0.0 => 0.0,
        None => return None,
        Some(width) if rect.size.width > width && rect.size.height > width => {
            let half = width * 0.5;
            half + (shape.radius() - half).max(0.0)
        }
        Some(_) => return None,
    };
    let (x0, x1) = (
        rect.origin.x + inset,
        rect.origin.x + rect.size.width - inset,
    );
    let (y0, y1) = (
        rect.origin.y + inset,
        rect.origin.y + rect.size.height - inset,
    );
    let (left, right) = ((a * x0 + e).min(a * x1 + e), (a * x0 + e).max(a * x1 + e));
    let (top, bottom) = ((d * y0 + f).min(d * y1 + f), (d * y0 + f).max(d * y1 + f));
    let edge = |value: f32| value.max(0.0) as usize;
    let inner = Bounds {
        left: edge(left.ceil() + 1.0),
        top: edge(top.ceil() + 1.0),
        right: edge(right.floor() - 1.0),
        bottom: edge(bottom.floor() - 1.0),
    };
    (!inner.is_empty()).then_some(inner)
}
