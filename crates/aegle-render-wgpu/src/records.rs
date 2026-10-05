//! CPU-side draw records: one storage row per primitive and one per clip scope.
use std::mem::size_of;

use aegle_scene::{Affine, RoundedRect};
use aegle_types::{Color, Point, Rect, color_math::linear_rgba};
use bytemuck::{Pod, Zeroable};

use crate::{Error, Result};

pub(crate) const NO_CLIP: u32 = u32::MAX;
const MAX_COORDINATE: f32 = 1_048_576.0;

/// Matches `Primitive` in shader.wgsl. `bounds` is already limited to the integer
/// scissor of its clip scope, so no per-draw scissor exists.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(crate) struct Primitive {
    pub bounds: [f32; 4],
    pub row0: [f32; 4],
    pub row1: [f32; 4],
    /// Shape rect, or atlas origin and glyph size excluding the gutter.
    pub rect: [f32; 4],
    /// Radius or mask contrast, stroke width (-1 fills) and viewport size.
    pub params: [f32; 4],
    pub color: [f32; 4],
    /// Clip head and atlas kind (1 mask, 2 color glyph, 3 image).
    pub header: [u32; 4],
}

/// One immutable clip, shared by all following draws in its scope.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(crate) struct Clip {
    pub row0: [f32; 4],
    pub row1: [f32; 4],
    pub rect: [f32; 4],
    pub extra: [u32; 4],
}

const _: () = assert!(size_of::<Primitive>() == 112 && size_of::<Clip>() == 64);

/// Which pipeline and texture draws a run of primitives.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Geometry,
    #[cfg(feature = "text")]
    Atlas(crate::atlas::Slot),
}

/// Adjacent primitives sharing a pipeline and page: one instanced draw.
pub(crate) struct Batch {
    pub kind: Kind,
    pub start: u32,
    pub end: u32,
}

#[derive(Default)]
pub(crate) struct Recording {
    pub primitives: Vec<Primitive>,
    pub batches: Vec<Batch>,
    /// Kept for the whole frame: later flushes still reference earlier scopes.
    pub clips: Vec<Clip>,
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub transform: Affine,
    pub clip: u32,
    pub bounds: [f32; 4],
}

struct LocalShape {
    row0: [f32; 4],
    row1: [f32; 4],
    rect: [f32; 4],
    radius: f32,
    stroke: f32,
}

impl Recording {
    /// Called when a frame starts; keeps allocations for reuse.
    pub fn clear(&mut self) {
        self.primitives.clear();
        self.batches.clear();
        self.clips.clear();
    }

    /// Called after a flush submitted the primitives; clips stay valid.
    pub fn clear_primitives(&mut self) {
        self.primitives.clear();
        self.batches.clear();
    }

    pub fn push_clip(
        &mut self,
        state: &mut State,
        shape: RoundedRect,
        transform: Affine,
    ) -> Result<()> {
        let [a, b, c, d, _, _] = transform.coefficients();
        let axis_aligned = (b == 0.0 && c == 0.0) || (a == 0.0 && d == 0.0);
        // Axis-aligned box coverage cannot reach pixels outside floor/ceil of
        // its edges. Keep a fringe only for the general affine AA approximation.
        let mut area = bounds(shape, transform, 0.0, if axis_aligned { 0.0 } else { 1.0 })?;
        if !shape.is_empty() {
            area = [
                area[0].floor(),
                area[1].floor(),
                area[2].ceil(),
                area[3].ceil(),
            ];
        }
        let local = local_shape(shape, transform, -1.0)?;
        let index = u32::try_from(self.clips.len()).map_err(|_| Error::Coordinates)?;
        if index == NO_CLIP {
            return Err(Error::Coordinates);
        }
        self.clips.push(Clip {
            row0: local.row0,
            row1: local.row1,
            rect: local.rect,
            extra: [local.radius.to_bits(), state.clip, 0, 0],
        });
        state.clip = index;
        state.bounds = intersection(state.bounds, area);
        Ok(())
    }

    /// Records a filled (`stroke < 0`) or centered-stroke rounded rectangle.
    pub fn shape(
        &mut self,
        state: State,
        shape: RoundedRect,
        color: Color,
        stroke: f32,
        viewport: [f32; 2],
    ) -> Result<()> {
        let area = bounds(shape, state.transform, stroke.max(0.0) * 0.5, 1.0)?;
        let local = local_shape(shape, state.transform, stroke)?;
        if color.to_rgba()[3] != 0 {
            self.record(
                Primitive {
                    bounds: area,
                    row0: local.row0,
                    row1: local.row1,
                    rect: local.rect,
                    params: [local.radius, local.stroke, viewport[0], viewport[1]],
                    color: linear_rgba(color.to_rgba()),
                    header: [state.clip, 0, 0, 0],
                },
                state.bounds,
                Kind::Geometry,
            );
        }
        Ok(())
    }

    /// Limits the quad to its integer clip-scope bounds, which covers exactly
    /// the pixel centers a scissor of the same rectangle would admit.
    pub fn record(&mut self, mut primitive: Primitive, clip: [f32; 4], kind: Kind) {
        let clipped = intersection(primitive.bounds, clip);
        if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
            return;
        }
        primitive.bounds = clipped;
        let end = self.primitives.len() as u32 + 1;
        match self.batches.last_mut() {
            Some(batch) if batch.kind == kind => batch.end = end,
            _ => self.batches.push(Batch {
                kind,
                start: end - 1,
                end,
            }),
        }
        self.primitives.push(primitive);
    }
}

fn rect_values(rect: Rect) -> [f32; 4] {
    [
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    ]
}

fn local_shape(shape: RoundedRect, transform: Affine, stroke: f32) -> Result<LocalShape> {
    let [x, y, w, h] = rect_values(shape.rect()).map(f64::from);
    // Normalize isotropically on the CPU so extreme local units cannot overflow
    // the shader's SDF squares or lose coverage thresholds.
    let scale = if shape.is_empty() {
        1.0
    } else {
        w.max(h).max(f64::from(stroke))
    };
    let [a, b, c, d, e, f] = transform
        .inverse()
        .map_err(|_| Error::Coordinates)?
        .coefficients()
        .map(f64::from);
    let row0 = [a / scale, c / scale, (e - x) / scale, 0.0].map(|v| v as f32);
    let row1 = [b / scale, d / scale, (f - y) / scale, 0.0].map(|v| v as f32);
    if row0.into_iter().chain(row1).any(|v| !v.is_finite()) {
        return Err(Error::Coordinates);
    }
    Ok(LocalShape {
        row0,
        row1,
        rect: [0.0, 0.0, (w / scale) as f32, (h / scale) as f32],
        radius: (f64::from(shape.radius()) / scale) as f32,
        stroke: if stroke < 0.0 {
            -1.0
        } else {
            (f64::from(stroke) / scale) as f32
        },
    })
}

/// Device-space bounds of `shape` under `transform`, widened by `outset` in local
/// units and `fringe` in device pixels.
pub(crate) fn bounds(
    shape: RoundedRect,
    transform: Affine,
    outset: f32,
    fringe: f32,
) -> Result<[f32; 4]> {
    let [x, y, w, h] = rect_values(shape.rect());
    let corners = [
        Point::new(x - outset, y - outset),
        Point::new(x + w + outset, y - outset),
        Point::new(x - outset, y + h + outset),
        Point::new(x + w + outset, y + h + outset),
    ]
    .map(|point| transform.map_point(point));
    if corners.iter().any(|point| {
        !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > MAX_COORDINATE
            || point.y.abs() > MAX_COORDINATE
    }) {
        return Err(Error::Coordinates);
    }
    let mut area = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for point in corners {
        area[0] = area[0].min(point.x);
        area[1] = area[1].min(point.y);
        area[2] = area[2].max(point.x);
        area[3] = area[3].max(point.y);
    }
    if shape.is_empty() {
        area[2] = area[0];
        area[3] = area[1];
    } else {
        // Cover the AA fringe even under reflection, rotation and shear.
        area[0] -= fringe;
        area[1] -= fringe;
        area[2] += fringe;
        area[3] += fringe;
    }
    Ok(area)
}

fn intersection(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}
