//! Storage rows and the geometry rules that fill them.
use std::mem::size_of;

use aegle_scene::{Affine, RoundedRect};
use aegle_types::{Color, Point, Rect, color_math::linear_rgba};
use bytemuck::{Pod, Zeroable};

use crate::{Error, Result};

/// Clip index meaning "no clip scope".
pub const NO_CLIP: u32 = u32::MAX;
const MAX_COORDINATE: f32 = 1_048_576.0;

/// Matches `Primitive` in the shared WGSL. `bounds` is already limited to the
/// integer scissor of its clip scope, so no per-draw scissor exists.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct Primitive {
    /// Device-space quad.
    pub bounds: [f32; 4],
    /// First row of the device-to-local affine.
    pub row0: [f32; 4],
    /// Second row of the device-to-local affine.
    pub row1: [f32; 4],
    /// Shape rect, or atlas origin and glyph size excluding the gutter.
    pub rect: [f32; 4],
    /// Radius or mask contrast, stroke width (-1 fills) and viewport size.
    pub params: [f32; 4],
    /// Linear premultiplied paint, or repeated color-glyph opacity.
    pub color: [f32; 4],
    /// Clip head, kind (0 geometry, 1 mask, 2 color glyph, 3 image), atlas page.
    pub header: [u32; 4],
}

/// One immutable clip, shared by all following draws in its scope.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct Clip {
    /// First row of the device-to-local affine.
    pub row0: [f32; 4],
    /// Second row of the device-to-local affine.
    pub row1: [f32; 4],
    /// Local rect.
    pub rect: [f32; 4],
    /// Radius bits and parent clip index.
    pub extra: [u32; 4],
}

const _: () = assert!(size_of::<Primitive>() == 112 && size_of::<Clip>() == 64);

/// Per-scope drawing state while walking a scene.
#[derive(Clone, Copy)]
pub struct State {
    /// Local-to-device transform.
    pub transform: Affine,
    /// Index of the innermost clip, or [`NO_CLIP`].
    pub clip: u32,
    /// Device bounds admitted by all enclosing clips.
    pub bounds: [f32; 4],
}

/// Clip and primitive rows for one frame, bounded by a byte limit.
pub struct Recording {
    /// Draw rows in painter's order.
    pub primitives: Vec<Primitive>,
    /// Clip rows; later primitives refer to them by index.
    pub clips: Vec<Clip>,
    /// Maximum capacity bytes of both vectors together; `usize::MAX` is unbounded.
    pub limit: usize,
}

impl Default for Recording {
    fn default() -> Self {
        Self::with_limit(usize::MAX)
    }
}

/// An atlas-backed draw: glyph, image or path mask.
pub struct Textured {
    /// Device-space quad.
    pub area: [f32; 4],
    /// Device-to-texel affine.
    pub inverse: Affine,
    /// Entry origin and size in its page, excluding any border.
    pub rect: [f32; 4],
    /// Mask contrast for glyphs, otherwise zero.
    pub contrast: f32,
    /// Viewport parameters from [`viewport`].
    pub viewport: [f32; 2],
    /// Linear premultiplied paint, or repeated color-glyph opacity.
    pub color: [f32; 4],
    /// 1 mask, 2 color glyph, 3 image.
    pub kind: u32,
    /// Backend page selector, ignored by the shader.
    pub page: u32,
}

impl Textured {
    /// Builds the storage row for `clip`.
    pub fn primitive(&self, clip: u32) -> Primitive {
        let [a, b, c, d, e, f] = self.inverse.coefficients();
        Primitive {
            bounds: self.area,
            row0: [a, c, e, 0.0],
            row1: [b, d, f, 0.0],
            rect: self.rect,
            params: [self.contrast, 0.0, self.viewport[0], self.viewport[1]],
            color: self.color,
            header: [clip, self.kind, self.page, 0],
        }
    }
}

/// Viewport parameters for the shader. `flip_y` selects WebGPU's upward clip space.
pub fn viewport(width: u32, height: u32, flip_y: bool) -> [f32; 2] {
    [
        width as f32,
        if flip_y {
            -(height as f32)
        } else {
            height as f32
        },
    ]
}

/// Whether device `area` reaches the integer clip `bounds` (fringe included).
pub fn visible(area: [f32; 4], bounds: [f32; 4]) -> bool {
    area[0].floor() < bounds[2]
        && area[2].ceil() > bounds[0]
        && area[1].floor() < bounds[3]
        && area[3].ceil() > bounds[1]
}

struct LocalShape {
    row0: [f32; 4],
    row1: [f32; 4],
    rect: [f32; 4],
    radius: f32,
    stroke: f32,
}

impl Recording {
    /// An empty recording that refuses to grow past `limit` bytes.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            primitives: Vec::new(),
            clips: Vec::new(),
            limit,
        }
    }

    /// Starts a frame; keeps allocations for reuse.
    pub fn clear(&mut self) {
        self.primitives.clear();
        self.clips.clear();
    }

    /// Opens a clip scope for `shape` under `transform`.
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
        reserve(&mut self.clips, &mut self.primitives, self.limit)?;
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
    /// Returns whether a row was added; transparent or clipped-out shapes add none.
    pub fn shape(
        &mut self,
        state: State,
        shape: RoundedRect,
        color: Color,
        stroke: f32,
        viewport: [f32; 2],
    ) -> Result<bool> {
        let area = bounds(shape, state.transform, stroke.max(0.0) * 0.5, 1.0)?;
        let local = local_shape(shape, state.transform, stroke)?;
        if color.to_rgba()[3] == 0 {
            return Ok(false);
        }
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
        )
    }

    /// Limits the quad to its integer clip-scope bounds, which covers exactly
    /// the pixel centers a scissor of the same rectangle would admit. Returns
    /// whether the row was added.
    pub fn record(&mut self, mut primitive: Primitive, clip: [f32; 4]) -> Result<bool> {
        let clipped = intersection(primitive.bounds, clip);
        if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
            return Ok(false);
        }
        primitive.bounds = clipped;
        reserve(&mut self.primitives, &mut self.clips, self.limit)?;
        self.primitives.push(primitive);
        Ok(true)
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

/// Device-space bounds of `shape` under `transform`, widened by `outset` in local
/// units and `fringe` in device pixels.
pub fn bounds(shape: RoundedRect, transform: Affine, outset: f32, fringe: f32) -> Result<[f32; 4]> {
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
