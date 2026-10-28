//! Offscreen layers composited at an opacity, and backdrop blur.
//!
//! A layer is a transparent premultiplied buffer covering its extent; draws
//! while it is open go there with coordinates shifted to its origin and its
//! own masks. Popping blends it onto the next target in linear light.

use aegle_scene::{Affine, BlurBox, Layer, Rect, blur_boxes, blur_reach};
use tiny_skia::{FillRule, Mask};

use crate::{
    RenderError, Renderer, Surface,
    blend::{blend_linear, blend_linear_span, premultiplied_linear},
    raster::{Bounds, Frame, rasterize},
};

/// An open layer: its pixels and where they land in the surface.
pub(crate) struct Open {
    pixels: Vec<u8>,
    masks: Vec<Mask>,
    /// Surface pixels the layer covers; empty when nothing of it can show.
    bounds: Bounds,
    /// Surface pixels the composite may change.
    clip: Bounds,
    opacity: f32,
}

impl Open {
    fn width(&self) -> usize {
        self.bounds.right - self.bounds.left
    }
    fn height(&self) -> usize {
        self.bounds.bottom - self.bounds.top
    }
}

/// `rect` moved by the translation of `shift`.
pub(crate) fn moved(rect: Rect, shift: Affine) -> Rect {
    let [.., x, y] = shift.coefficients();
    Rect::new(
        rect.origin.x + x,
        rect.origin.y + y,
        rect.size.width,
        rect.size.height,
    )
}

/// Surface pixels whose centers `rect` contains, as a scissor rounds.
fn pixels(rect: Rect, surface: &Surface<'_>) -> Bounds {
    let edge = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as usize;
    let bounds = Bounds {
        left: edge(rect.origin.x, surface.width),
        top: edge(rect.origin.y, surface.height),
        right: edge(rect.origin.x + rect.size.width, surface.width),
        bottom: edge(rect.origin.y + rect.size.height, surface.height),
    };
    if bounds.is_empty() {
        Bounds::EMPTY
    } else {
        bounds
    }
}

/// Bounds made of the surface's whole pixel rectangle `rect` (already whole).
fn whole(rect: Rect) -> Bounds {
    Bounds {
        left: rect.origin.x as usize,
        top: rect.origin.y as usize,
        right: (rect.origin.x + rect.size.width) as usize,
        bottom: (rect.origin.y + rect.size.height) as usize,
    }
}

impl Renderer {
    /// Limits the bytes of open layer images and blur scratch. Opening a
    /// layer beyond it fails with [`RenderError::EffectBudget`]; a backdrop
    /// blur beyond it is skipped and counted by [`Self::skipped_blurs`]. The
    /// default is unlimited, since layers are at most the window's size.
    pub fn set_effect_budget(&mut self, bytes: usize) {
        self.effect_budget = bytes;
    }

    /// Backdrop blurs skipped so far for exceeding the effect budget.
    pub fn skipped_blurs(&self) -> u64 {
        self.skipped_blurs
    }
}

impl Frame<'_, '_, '_> {
    /// Opens a layer: until the matching [`Self::pop_layer`], draws go to an
    /// offscreen image covering the layer's extent within the frame region,
    /// composited on pop. A positive backdrop blur first blurs what is
    /// already drawn under the layer's shape. Blurring reads pixels up to
    /// about three standard deviations around the shape, so a region frame
    /// ([`Renderer::begin_region`]) must contain that whole area, or it reads
    /// the previous frame's result there. Dropping the frame composites
    /// layers left open.
    pub fn push_layer(&mut self, layer: &Layer) -> Result<(), RenderError> {
        if layer.backdrop_blur() > 0.0 {
            self.in_target(|frame, shift| frame.blur_backdrop(layer, shift))?;
        }
        let (bounds, clip) = self.in_target(|frame, shift| {
            let [.., x, y] = shift.coefficients();
            let [x, y] = [-x as usize, -y as usize];
            let region = whole(frame.region);
            let clip = layer.clip().map_or(region, |rect| {
                pixels(moved(rect, shift), frame.surface).intersect(region)
            });
            let extent = pixels(moved(layer.extent(), shift), frame.surface).intersect(clip);
            let absolute = |b: Bounds| Bounds {
                left: b.left + x,
                top: b.top + y,
                right: b.right + x,
                bottom: b.bottom + y,
            };
            Ok((absolute(extent), absolute(clip)))
        })?;
        let bytes = (bounds.right - bounds.left) * (bounds.bottom - bounds.top) * 4;
        let held: usize = self.layers.iter().map(|open| open.pixels.len()).sum();
        let required = held.saturating_add(if bounds.is_empty() { 0 } else { bytes });
        if required > self.renderer.effect_budget {
            return Err(RenderError::EffectBudget {
                required,
                limit: self.renderer.effect_budget,
            });
        }
        let mut pixels = self.renderer.spare.pop().unwrap_or_default();
        pixels.clear();
        if !bounds.is_empty() {
            pixels
                .try_reserve_exact(bytes)
                .map_err(|_| RenderError::Allocation)?;
            pixels.resize(bytes, 0);
        }
        self.layers.push(Open {
            pixels,
            masks: Vec::new(),
            bounds,
            clip,
            opacity: layer.opacity(),
        });
        Ok(())
    }

    /// Closes the innermost layer and composites it onto the target beneath.
    pub fn pop_layer(&mut self) -> Result<(), RenderError> {
        let open = self.layers.pop().ok_or(RenderError::UnbalancedLayer)?;
        self.composite(&open);
        self.renderer.spare.push(open.pixels);
        Ok(())
    }

    fn composite(&mut self, open: &Open) {
        if open.bounds.is_empty() || open.opacity == 0.0 {
            return;
        }
        let (width, opacity) = (open.width(), open.opacity);
        let _ = self.in_target(|frame, shift| {
            let [.., x, y] = shift.coefficients();
            let [x, y] = [-x as usize, -y as usize];
            let area = open.bounds.intersect(open.clip);
            let stride = frame.surface.width as usize;
            for row in area.top..area.bottom {
                let source = (row - open.bounds.top) * width + area.left - open.bounds.left;
                let target = (row - y) * stride + area.left - x;
                let count = area.right - area.left;
                let from = open.pixels[source * 4..(source + count) * 4].as_chunks().0;
                let to = &mut frame.surface.data[target * 4..(target + count) * 4];
                blend_linear_span(to, 0, |index, pixel| {
                    let source: [u8; 4] = from[index];
                    if source[3] == 255 && opacity == 1.0 {
                        pixel.copy_from_slice(&source);
                        return ([0.0; 4], 0);
                    }
                    let linear = premultiplied_linear(source).map(|c| c * opacity);
                    (linear, 255)
                });
            }
            Ok(())
        });
    }

    /// Runs `draw` on the innermost open layer, as a frame of its own with
    /// its masks, or on this frame; `shift` maps surface to target pixels.
    pub(crate) fn in_target<T>(
        &mut self,
        draw: impl FnOnce(&mut Frame<'_, '_, '_>, Affine) -> Result<T, RenderError>,
    ) -> Result<T, RenderError> {
        let Some(open) = self.layers.last_mut() else {
            return draw(self, Affine::IDENTITY);
        };
        let (left, top) = (open.bounds.left as f32, open.bounds.top as f32);
        let shift = Affine::translation(-left, -top);
        if open.bounds.is_empty() {
            // Nothing of this layer can show: draw into an empty region.
            let mut none = [0u8; 4];
            let mut surface = Surface::new(&mut none, 1, 1)?;
            let renderer = &mut *self.renderer;
            let mut frame = Frame {
                renderer,
                surface: &mut surface,
                region: Rect::new(0.0, 0.0, 0.0, 0.0),
                layers: Vec::new(),
            };
            return draw(&mut frame, shift);
        }
        let (width, height) = (open.width() as u32, open.height() as u32);
        let mut surface = Surface::new(&mut open.pixels, width, height)?;
        surface.bgra = self.surface.bgra;
        std::mem::swap(&mut self.renderer.masks, &mut open.masks);
        let result = {
            let mut frame = Frame {
                renderer: &mut *self.renderer,
                surface: &mut surface,
                region: Rect::new(0.0, 0.0, width as f32, height as f32),
                layers: Vec::new(),
            };
            draw(&mut frame, shift)
        };
        std::mem::swap(&mut self.renderer.masks, &mut open.masks);
        result
    }

    /// Draws the blur of this target's pixels under the layer shape over
    /// them, at the layer's opacity.
    fn blur_backdrop(&mut self, layer: &Layer, shift: Affine) -> Result<(), RenderError> {
        let Some(boxes) = blur_boxes(layer.target_blur()) else {
            return Ok(());
        };
        let region = whole(self.region);
        let mut area = pixels(moved(layer.shape_bounds(), shift), self.surface).intersect(region);
        if let Some(clip) = layer.clip() {
            area = area.intersect(pixels(moved(clip, shift), self.surface));
        }
        if area.is_empty() {
            return Ok(());
        }
        let reach = blur_reach(&boxes) as usize;
        let (width, height) = (self.surface.width as usize, self.surface.height as usize);
        let sampled = Bounds {
            left: area.left.saturating_sub(reach),
            top: area.top.saturating_sub(reach),
            right: (area.right + reach).min(width),
            bottom: (area.bottom + reach).min(height),
        };
        let (columns, rows) = (sampled.right - sampled.left, sampled.bottom - sampled.top);
        let held: usize = self.layers.iter().map(|open| open.pixels.len()).sum();
        let bytes = columns * rows * 2 * size_of::<[f32; 4]>();
        if held.saturating_add(bytes) > self.renderer.effect_budget {
            self.renderer.skipped_blurs += 1;
            return Ok(());
        }
        let [mut image, mut scratch] = std::mem::take(&mut self.renderer.blur);
        for buffer in [&mut image, &mut scratch] {
            buffer.clear();
            buffer
                .try_reserve_exact(columns * rows)
                .map_err(|_| RenderError::Allocation)?;
        }
        for row in sampled.rows(width) {
            let pixels = self.surface.data[row.start * 4..row.end * 4].as_chunks().0;
            image.extend(pixels.iter().map(|&pixel| premultiplied_linear(pixel)));
        }
        scratch.resize(columns * rows, [0.0; 4]);
        for pass in boxes {
            box_pass(&image, &mut scratch, columns, rows, 1, columns, pass);
            std::mem::swap(&mut image, &mut scratch);
        }
        for pass in boxes {
            box_pass(&image, &mut scratch, rows, columns, columns, 1, pass);
            std::mem::swap(&mut image, &mut scratch);
        }
        // The shape's coverage, then the blurred pixels over the backdrop.
        self.renderer.reserve_masks(1, self.surface)?;
        let transform = layer
            .transform()
            .then(shift)
            .map_err(|_| RenderError::Coordinates)?;
        let path = self.geometry(layer.shape(), None, transform)?;
        rasterize(&mut self.renderer.masks[0], &path, area, FillRule::Winding);
        self.renderer.path = path.clear();
        let (mask, opacity) = (self.renderer.masks[0].data(), layer.opacity());
        for y in area.top..area.bottom {
            for x in area.left..area.right {
                let coverage = mask[y * width + x];
                let blurred =
                    image[(y - sampled.top) * columns + x - sampled.left].map(|c| c * opacity);
                let pixel = (y * width + x) * 4;
                blend_linear(&mut self.surface.data[pixel..pixel + 4], blurred, coverage);
            }
        }
        self.renderer.blur = [image, scratch];
        Ok(())
    }
}

impl Drop for Frame<'_, '_, '_> {
    fn drop(&mut self) {
        while let Some(open) = self.layers.pop() {
            self.composite(&open);
        }
    }
}

/// One box blur along lines of `length` pixels, `lines` of them, where
/// consecutive pixels of a line are `step` apart and lines `stride` apart;
/// reads beyond a line's ends repeat its edge pixels.
fn box_pass(
    source: &[[f32; 4]],
    target: &mut [[f32; 4]],
    length: usize,
    lines: usize,
    step: usize,
    stride: usize,
    pass: BlurBox,
) {
    let (left, size) = (pass.left as isize, pass.size as isize);
    let scale = 1.0 / size as f32;
    let last = length as isize - 1;
    for line in 0..lines {
        let at = |i: isize| source[line * stride + i.clamp(0, last) as usize * step];
        let mut sum = [0.0f32; 4];
        for i in -left..size - left {
            let pixel = at(i);
            for c in 0..4 {
                sum[c] += pixel[c];
            }
        }
        for x in 0..length as isize {
            target[line * stride + x as usize * step] = sum.map(|c| c * scale);
            let (enter, leave) = (at(x + size - left), at(x - left));
            for c in 0..4 {
                sum[c] += enter[c] - leave[c];
            }
        }
    }
}
