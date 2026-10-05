use std::mem::size_of;

use aegle_scene::{Affine, Command, RoundedRect, Scene};
use aegle_types::{Color, Point, Rect, color_math::linear_rgba};
use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::{Error, Result};

const NO_CLIP: u32 = u32::MAX;
const MAX_COORDINATE: f32 = 1_048_576.0;

/// Matches the 112-byte push-constant block in geometry.wgsl.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub(crate) struct Primitive {
    pub bounds: [f32; 4],
    pub row0: [f32; 4],
    pub row1: [f32; 4],
    pub rect: [f32; 4],
    pub params: [f32; 4],
    pub color: [f32; 4],
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

pub(crate) struct Draw {
    pub primitive: Primitive,
    pub scissor: vk::Rect2D,
}

#[derive(Default)]
pub(crate) struct Recording {
    pub draws: Vec<Draw>,
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
    pub fn clear(&mut self) {
        self.draws.clear();
        self.clips.clear();
    }

    pub fn append(
        &mut self,
        scene: &Scene,
        transform: Affine,
        clip: Option<Rect>,
        width: u32,
        height: u32,
        byte_limit: usize,
        #[cfg(feature = "text")] mut text: impl FnMut(
            &mut Self,
            &aegle_scene::GlyphRun,
            State,
        ) -> Result,
    ) -> Result<()> {
        if scene.max_clip_depth() + usize::from(clip.is_some()) > 8 {
            return Err(Error::ClipDepth);
        }
        if scene.is_empty() {
            if let Some(rect) = clip {
                bounds(RoundedRect::new(rect, 0.0)?, Affine::IDENTITY, 0.0, 0.0)?;
            }
            return Ok(());
        }
        let mut state = State {
            transform,
            clip: NO_CLIP,
            bounds: [0.0, 0.0, width as f32, height as f32],
        };
        if let Some(rect) = clip {
            let shape = RoundedRect::new(rect, 0.0)?;
            self.push_clip(&mut state, shape, Affine::IDENTITY, byte_limit)?;
        }
        // Scene scopes have a validated maximum of 64. No per-append heap scratch.
        let mut saved = [state; aegle_scene::MAX_SCOPE_DEPTH];
        let mut depth = 0;
        for command in scene.commands() {
            match *command {
                Command::PushTransform(local) => {
                    saved[depth] = state;
                    depth += 1;
                    state.transform = local.then(state.transform)?;
                }
                Command::PushClip(shape) => {
                    saved[depth] = state;
                    depth += 1;
                    let transform = state.transform;
                    self.push_clip(&mut state, shape, transform, byte_limit)?;
                }
                Command::Pop => {
                    depth -= 1;
                    state = saved[depth];
                }
                Command::Fill { shape, color } => {
                    self.draw(state, shape, color, -1.0, width, height, byte_limit)?;
                }
                Command::Stroke {
                    shape,
                    color,
                    width: stroke,
                } => {
                    self.draw(state, shape, color, stroke, width, height, byte_limit)?;
                }
                #[cfg(feature = "text")]
                Command::Glyphs(index) => text(self, &scene.glyph_runs()[index], state)?,
                _ => return Err(Error::UnsupportedCommand),
            }
        }
        Ok(())
    }

    fn push_clip(
        &mut self,
        state: &mut State,
        shape: RoundedRect,
        transform: Affine,
        limit: usize,
    ) -> Result<()> {
        let [a, b, c, d, _, _] = transform.coefficients();
        let axis_aligned = (b == 0.0 && c == 0.0) || (a == 0.0 && d == 0.0);
        // Axis-aligned box coverage cannot reach pixels outside floor/ceil of
        // its edges. Keep a fringe only for the general affine AA approximation.
        let mut bounds = bounds(shape, transform, 0.0, if axis_aligned { 0.0 } else { 1.0 })?;
        if !shape.is_empty() {
            bounds = [
                bounds[0].floor(),
                bounds[1].floor(),
                bounds[2].ceil(),
                bounds[3].ceil(),
            ];
        }
        let local = local_shape(shape, transform, -1.0)?;
        reserve(&mut self.clips, &mut self.draws, limit)?;
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
        state.bounds = intersection(state.bounds, bounds);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        state: State,
        shape: RoundedRect,
        color: Color,
        stroke: f32,
        width: u32,
        height: u32,
        limit: usize,
    ) -> Result<()> {
        let bounds = bounds(shape, state.transform, stroke.max(0.0) * 0.5, 1.0)?;
        let local = local_shape(shape, state.transform, stroke)?;
        if color.to_rgba()[3] == 0 {
            return Ok(());
        }
        self.record(
            Primitive {
                bounds,
                row0: local.row0,
                row1: local.row1,
                rect: local.rect,
                params: [local.radius, local.stroke, width as f32, height as f32],
                color: linear_rgba(color.to_rgba()),
                header: [state.clip, 0, 0, 0],
            },
            state.bounds,
            width,
            height,
            limit,
        )
    }

    pub fn record(
        &mut self,
        primitive: Primitive,
        clip: [f32; 4],
        width: u32,
        height: u32,
        limit: usize,
    ) -> Result {
        let clipped = intersection(primitive.bounds, clip);
        if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
            return Ok(());
        }
        let left = clipped[0].floor().max(0.0) as i32;
        let top = clipped[1].floor().max(0.0) as i32;
        let right = clipped[2].ceil().min(width as f32) as i32;
        let bottom = clipped[3].ceil().min(height as f32) as i32;
        reserve(&mut self.draws, &mut self.clips, limit)?;
        self.draws.push(Draw {
            primitive,
            scissor: vk::Rect2D {
                offset: vk::Offset2D { x: left, y: top },
                extent: vk::Extent2D {
                    width: (right - left) as u32,
                    height: (bottom - top) as u32,
                },
            },
        });
        Ok(())
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
    // Normalize isotropically on the CPU. Large/tiny local units can map to an
    // ordinary device rectangle; shader SDF squares and AA thresholds must not
    // overflow or lose coverage merely because its original units were extreme.
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
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for point in corners {
        bounds[0] = bounds[0].min(point.x);
        bounds[1] = bounds[1].min(point.y);
        bounds[2] = bounds[2].max(point.x);
        bounds[3] = bounds[3].max(point.y);
    }
    if shape.is_empty() {
        bounds[2] = bounds[0];
        bounds[3] = bounds[1];
    } else {
        // Cover the AA fringe even under reflection, rotation and shear.
        bounds[0] -= fringe;
        bounds[1] -= fringe;
        bounds[2] += fringe;
        bounds[3] += fringe;
    }
    Ok(bounds)
}

fn intersection(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

/// Grow only the exhausted array, keeping both retained capacities in budget.
fn reserve<T, U>(values: &mut Vec<T>, other: &mut Vec<U>, limit: usize) -> Result<()> {
    if values.len() < values.capacity() {
        return Ok(());
    }
    let next_bytes = values
        .len()
        .saturating_add(1)
        .saturating_mul(size_of::<T>());
    let mut other_bytes = other.capacity() * size_of::<U>();
    if next_bytes.saturating_add(other_bytes) > limit {
        // Reclaim unused growth in the other array before rejecting real work.
        other.shrink_to_fit();
        other_bytes = other.capacity() * size_of::<U>();
    }
    let required = next_bytes.saturating_add(other_bytes);
    if required > limit {
        return Err(Error::Budget {
            required: required as u64,
            limit: limit as u64,
        });
    }
    let max_capacity = (limit - other_bytes) / size_of::<T>();
    let capacity = values
        .capacity()
        .saturating_mul(2)
        .max(16)
        .min(max_capacity);
    values
        .try_reserve_exact(capacity - values.len())
        .map_err(|_| Error::Allocation)
}
