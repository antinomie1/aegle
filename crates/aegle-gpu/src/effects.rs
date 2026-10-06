//! Gradient fills and analytic shadows, drawn by the geometry shader.
use aegle_scene::{Gradient, GradientGeometry, RoundedRect};
use aegle_types::{Color, Point, color_math::linear_rgba};

use crate::{
    Clip, Error, Primitive, Recording, Result, State,
    records::{bounds, local_shape, reserve},
};

const LINEAR: u32 = 1;
const RADIAL: u32 = 2;
const SHADOW: u32 = 3;

impl Recording {
    /// Records `shape` filled with `gradient`, appending its stops to the clip
    /// rows. Returns whether a row was added.
    pub fn gradient(
        &mut self,
        state: State,
        shape: RoundedRect,
        gradient: &Gradient,
        viewport: [f32; 2],
    ) -> Result<bool> {
        let area = bounds(shape, state.transform, 0.0, 1.0)?;
        if !crate::visible(area, state.bounds) {
            return Ok(false);
        }
        let local = local_shape(shape, state.transform, -1.0)?;
        let origin = shape.rect().origin;
        let normalize = |point: Point| {
            [
                (point.x - origin.x) / local.scale,
                (point.y - origin.y) / local.scale,
            ]
        };
        let (effect, paint, extent) = match gradient.geometry() {
            GradientGeometry::Linear { start, end } => {
                let ([x0, y0], [x1, y1]) = (normalize(start), normalize(end));
                (LINEAR, [x0, y0, x1, y1], (x1 - x0).hypot(y1 - y0))
            }
            GradientGeometry::Radial { center, radius } => {
                let [x, y] = normalize(center);
                let radius = radius / local.scale;
                (RADIAL, [x, y, radius, 0.0], radius)
            }
        };
        // Normalizing must neither overflow nor collapse the gradient's extent.
        if !paint.iter().all(|value| value.is_finite()) || !extent.is_normal() {
            return Err(Error::Coordinates);
        }
        let first = u32::try_from(self.clips.len()).map_err(|_| Error::Coordinates)?;
        for pair in gradient.stops().chunks(2) {
            let color = |index: usize| {
                pair.get(index)
                    .map_or([0.0; 4], |stop| linear_rgba(stop.color.to_rgba()))
            };
            let offset = |index: usize| pair.get(index).map_or(1.0, |stop| stop.offset);
            reserve(&mut self.clips, &mut self.primitives, self.limit)?;
            self.clips.push(Clip {
                row0: color(0),
                row1: color(1),
                rect: [offset(0), offset(1), 0.0, 0.0],
                extra: [0; 4],
            });
        }
        let count = gradient.stops().len() as u32;
        self.record(
            Primitive {
                bounds: area,
                row0: local.row0,
                row1: local.row1,
                rect: local.rect,
                params: [local.radius, -1.0, viewport[0], viewport[1]],
                color: paint,
                header: [state.clip, 0, first, effect | count << 8],
            },
            state.bounds,
        )
    }

    /// Records the Gaussian shadow of `shape` with standard deviation `blur`
    /// in local units. Returns whether a row was added.
    pub fn shadow(
        &mut self,
        state: State,
        shape: RoundedRect,
        color: Color,
        blur: f32,
        viewport: [f32; 2],
    ) -> Result<bool> {
        let area = bounds(shape, state.transform, blur * 3.0, 1.0)?;
        let local = local_shape(shape, state.transform, -1.0)?;
        if color.to_rgba()[3] == 0 {
            return Ok(false);
        }
        self.record(
            Primitive {
                bounds: area,
                row0: local.row0,
                row1: local.row1,
                rect: local.rect,
                params: [local.radius, blur / local.scale, viewport[0], viewport[1]],
                color: linear_rgba(color.to_rgba()),
                header: [state.clip, 0, 0, SHADOW],
            },
            state.bounds,
        )
    }
}
