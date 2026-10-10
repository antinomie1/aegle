//! Pixel plans for layers and backdrop blur, and the textured draws that
//! composite them, shared by both GPU renderers.
//!
//! A backend renders a layer's content into its own linear image covering
//! [`LayerPlan::extent`], with draws shifted to that image's origin. For a
//! backdrop blur it copies [`BlurPlan::sampled`] of the current target into a
//! scratch image, runs the six box passes of [`BLUR_WGSL`] (entries `vs_main`
//! and `fs_main`, one draw each, parameters in the instance index), and draws
//! the result back through [`composite`] clipped to the layer shape.

use aegle_scene::{Affine, Layer, Rect, RoundedRect, blur_boxes, blur_reach};

use crate::{Recording, Result, State, Textured, records::NO_CLIP};

/// Box blur pass shader: samples `source` along one axis.
pub const BLUR_WGSL: &str = include_str!("blur.wgsl");

/// Target pixels `[left, top, right, bottom]`, empty when `left >= right`
/// or `top >= bottom`.
pub type Pixels = [u32; 4];

/// The pixels a rect covers, rounded to the nearest edges like a scissor and
/// limited to a `size` target.
pub fn pixels(rect: Rect, size: [u32; 2]) -> Pixels {
    let edge = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
    [
        edge(rect.origin.x, size[0]),
        edge(rect.origin.y, size[1]),
        edge(rect.origin.x + rect.size.width, size[0]),
        edge(rect.origin.y + rect.size.height, size[1]),
    ]
}

/// Whether `pixels` covers nothing.
pub fn empty(pixels: Pixels) -> bool {
    pixels[0] >= pixels[2] || pixels[1] >= pixels[3]
}

fn intersect(a: Pixels, b: Pixels) -> Pixels {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

/// `rect` moved by `-origin`, into a target whose top left is at `origin`.
pub fn shifted(rect: Rect, origin: [u32; 2]) -> Rect {
    Rect::new(
        rect.origin.x - origin[0] as f32,
        rect.origin.y - origin[1] as f32,
        rect.size.width,
        rect.size.height,
    )
}

/// Where a layer's image goes in the target it opens on.
#[derive(Clone, Copy, Debug)]
pub struct LayerPlan {
    /// Target pixels the image covers; empty when nothing of it can show.
    pub extent: Pixels,
    /// Target pixels the composite may change.
    pub clip: Pixels,
}

/// Plans a layer opening on a `size` target whose top left lies at `origin`
/// in the coordinates `layer` uses.
pub fn plan_layer(layer: &Layer, origin: [u32; 2], size: [u32; 2]) -> LayerPlan {
    let all = [0, 0, size[0], size[1]];
    let clip = layer
        .clip()
        .map_or(all, |clip| pixels(shifted(clip, origin), size));
    let extent = intersect(pixels(shifted(layer.extent(), origin), size), clip);
    LayerPlan { extent, clip }
}

/// One backdrop blur in a target.
#[derive(Clone, Copy, Debug)]
pub struct BlurPlan {
    /// Target pixels the blurred result may change.
    pub area: Pixels,
    /// Target pixels the passes read: the area grown by the blur's reach.
    pub sampled: Pixels,
    /// Instance index of each box pass, horizontal first.
    pub passes: [u32; 6],
}

/// Plans the backdrop blur of `layer` in a `size` target at `origin`, or
/// `None` when it changes nothing.
pub fn plan_blur(layer: &Layer, origin: [u32; 2], size: [u32; 2]) -> Option<BlurPlan> {
    let boxes = blur_boxes(layer.target_blur())?;
    let mut area = pixels(shifted(layer.shape_bounds(), origin), size);
    if let Some(clip) = layer.clip() {
        area = intersect(area, pixels(shifted(clip, origin), size));
    }
    if empty(area) {
        return None;
    }
    let reach = blur_reach(&boxes);
    let sampled = [
        area[0].saturating_sub(reach),
        area[1].saturating_sub(reach),
        (area[2] + reach).min(size[0]),
        (area[3] + reach).min(size[1]),
    ];
    let pass = |axis: u32, index: usize| {
        let b = boxes[index];
        axis << 31 | (b.size & 0x7fff) << 16 | (b.left & 0xffff)
    };
    let passes = [0, 1, 2].map(|i| pass(0, i));
    let passes = [
        passes[0],
        passes[1],
        passes[2],
        pass(1, 0),
        pass(1, 1),
        pass(1, 2),
    ];
    Some(BlurPlan {
        area,
        sampled,
        passes,
    })
}

/// Records an image of `size` texels drawn one texel per pixel at target
/// pixel `at`, multiplied by `opacity`, inside `clip` pixels and, when
/// given, a shape under its transform. `page` selects the backend's texture.
#[allow(clippy::too_many_arguments)]
pub fn composite(
    recording: &mut Recording,
    size: [u32; 2],
    at: [u32; 2],
    clip: Pixels,
    shape: Option<(RoundedRect, Affine)>,
    opacity: f32,
    viewport: [f32; 2],
    page: u32,
) -> Result<()> {
    if empty(clip) || opacity == 0.0 {
        return Ok(());
    }
    let scissor = Rect::new(
        clip[0] as f32,
        clip[1] as f32,
        (clip[2] - clip[0]) as f32,
        (clip[3] - clip[1]) as f32,
    );
    let mut state = State {
        transform: Affine::IDENTITY,
        clip: NO_CLIP,
        bounds: [0.0, 0.0, f32::MAX, f32::MAX],
    };
    recording.push_clip(&mut state, RoundedRect::new(scissor, 0.0), Affine::IDENTITY)?;
    if let Some((shape, transform)) = shape {
        recording.push_clip(&mut state, shape, transform)?;
    }
    let (x, y) = (at[0] as f32, at[1] as f32);
    let area = [x, y, x + size[0] as f32, y + size[1] as f32];
    let inverse = Affine::translation(-x, -y);
    recording.record(
        Textured {
            area,
            inverse,
            rect: [0.0, 0.0, size[0] as f32, size[1] as f32],
            contrast: 0.0,
            viewport,
            color: [opacity; 4],
            kind: 3,
            page,
        }
        .primitive(state.clip),
        state.bounds,
    )?;
    Ok(())
}
