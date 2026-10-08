use aegle_scene::{Affine, Command, RoundedRect, Scene};
use aegle_types::{Color, Rect};
use tiny_skia::{FillRule, IntSize, Mask, Path, PathBuilder, Transform};

pub(crate) use crate::bounds::Bounds;
use crate::{RenderError, Surface, blend::Solid, path};

/// Reusable CPU rendering state with an optional coverage/clip mask budget.
///
/// Shape/clip draws reserve one byte per surface pixel for coverage, plus one
/// byte per pixel per nested clip; clips nest at most eight deep, as on the GPU
/// backends, so masks never exceed nine bytes per pixel. Text without clips
/// needs no surface-sized mask. The default budget is unlimited, since the
/// surface size is the window's; a byte limit caps masks on small devices.
/// Masks are retained for the largest clip depth at the current surface size. Path/stack storage and the
/// rasterizer's temporary scanline buffers are not included in this budget.
/// Devices without a GPU can use this crate without any platform library.
pub struct Renderer {
    mask_budget: usize,
    /// Byte limit of layer images and blur scratch.
    pub(crate) effect_budget: usize,
    /// Pixel storage of finished layers, reused by later ones.
    pub(crate) spare: Vec<Vec<u8>>,
    /// Blur scratch: two premultiplied linear buffers of the sampled area.
    pub(crate) blur: [Vec<[f32; 4]>; 2],
    /// Backdrop blurs skipped for exceeding the effect budget.
    pub(crate) skipped_blurs: u64,
    pub(crate) masks: Vec<Mask>,
    #[cfg(feature = "text")]
    pub(crate) glyphs: aegle_glyph::GlyphCache,
    dimensions: (u32, u32),
    stack: Vec<State>,
    pub(crate) path: PathBuilder,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new(usize::MAX)
    }
}

impl Renderer {
    /// Creates an allocation-free renderer with a mask limit in bytes.
    pub fn new(mask_budget: usize) -> Self {
        Self {
            mask_budget,
            effect_budget: usize::MAX,
            spare: Vec::new(),
            blur: [Vec::new(), Vec::new()],
            skipped_blurs: 0,
            masks: Vec::new(),
            #[cfg(feature = "text")]
            glyphs: aegle_glyph::GlyphCache::default(),
            dimensions: (0, 0),
            stack: Vec::new(),
            path: PathBuilder::new(),
        }
    }

    /// Clears the framebuffer and starts a complete frame.
    ///
    /// Submit retained records in painter order through [`Frame::draw`]. Dropping
    /// the frame releases the surface borrow; it does not present or schedule
    /// work. Skip this call entirely when the UI has no pixel changes.
    pub fn begin_frame<'r, 's, 'p>(
        &'r mut self,
        surface: &'s mut Surface<'p>,
        clear: Color,
    ) -> Frame<'r, 's, 'p> {
        let all = Rect::new(0.0, 0.0, surface.width as f32, surface.height as f32);
        self.begin_region(surface, clear, all)
    }

    /// Like [`Self::begin_frame`], but clears and draws only inside `region`,
    /// leaving the other pixels of a retained framebuffer as they are. The
    /// region is in device pixels; its edges are rounded outward to whole
    /// pixels and it is limited to the surface, so it never needs a mask.
    pub fn begin_region<'r, 's, 'p>(
        &'r mut self,
        surface: &'s mut Surface<'p>,
        clear: Color,
        region: Rect,
    ) -> Frame<'r, 's, 'p> {
        if self.dimensions != (surface.width, surface.height) {
            self.masks.clear();
            self.dimensions = (surface.width, surface.height);
        }
        let edge = |value: f32, limit: u32| value.clamp(0.0, limit as f32);
        let (left, top) = (
            edge(region.origin.x.floor(), surface.width),
            edge(region.origin.y.floor(), surface.height),
        );
        let right = edge((region.origin.x + region.size.width).ceil(), surface.width);
        let bottom = edge(
            (region.origin.y + region.size.height).ceil(),
            surface.height,
        );
        let region = Rect::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0));
        surface.clear(
            clear,
            [left, top, right.max(left), bottom.max(top)].map(|v| v as usize),
        );
        Frame {
            renderer: self,
            surface,
            region,
            layers: Vec::new(),
        }
    }

    /// Actual bytes held by coverage/clip mask pixel arrays, excluding metadata.
    pub fn allocated_mask_bytes(&self) -> usize {
        self.masks.iter().map(|mask| mask.data().len()).sum()
    }

    /// On-demand glyph cache statistics and configuration, separate from masks.
    #[cfg(feature = "text")]
    pub fn glyph_cache(&self) -> &aegle_glyph::GlyphCache {
        &self.glyphs
    }

    /// Accesses the glyph cache to clear it or replace it with different limits.
    #[cfg(feature = "text")]
    pub fn glyph_cache_mut(&mut self) -> &mut aegle_glyph::GlyphCache {
        &mut self.glyphs
    }

    /// Releases reusable masks, paths and traversal storage when memory is needed.
    /// The next drawing operation allocates again. Transfer LUTs remain shared.
    /// Glyph scaling scratch is released, but completed glyph images remain;
    /// clear them separately through `glyph_cache_mut().clear()` with `text`.
    pub fn release_scratch(&mut self) {
        self.masks = Vec::new();
        self.spare = Vec::new();
        self.blur = [Vec::new(), Vec::new()];
        self.stack = Vec::new();
        self.path = PathBuilder::new();
        #[cfg(feature = "text")]
        self.glyphs.release_scratch();
    }

    fn prepare(
        &mut self,
        scene: &Scene,
        surface: &Surface<'_>,
        external_clip: bool,
    ) -> Result<(), RenderError> {
        let depth = scene.max_clip_depth() + usize::from(external_clip);
        if depth > 8 {
            return Err(RenderError::ClipDepth);
        }
        let needs_masks = depth > 0
            || scene.commands().iter().any(|command| {
                matches!(
                    command,
                    Command::Fill { .. }
                        | Command::Stroke { .. }
                        | Command::FillGradient { .. }
                        | Command::FillPath { .. }
                        | Command::StrokePath { .. }
                )
            });
        self.reserve_masks(if needs_masks { depth + 1 } else { 0 }, surface)?;
        self.stack.clear();
        self.stack.reserve(scene.max_depth());
        Ok(())
    }

    /// Ensures `count` surface-sized masks within the mask budget.
    pub(crate) fn reserve_masks(
        &mut self,
        count: usize,
        surface: &Surface<'_>,
    ) -> Result<(), RenderError> {
        let pixels = surface.data.len() / 4;
        let required = pixels.saturating_mul(count);
        if required > self.mask_budget {
            return Err(RenderError::MaskBudget {
                required,
                limit: self.mask_budget,
            });
        }
        while self.masks.len() < count {
            let mut data = Vec::new();
            data.try_reserve_exact(pixels)
                .map_err(|_| RenderError::Allocation)?;
            data.resize(pixels, 0);
            let size = IntSize::from_wh(surface.width, surface.height).unwrap();
            self.masks.push(Mask::from_vec(data, size).unwrap());
        }
        Ok(())
    }
}

/// A complete frame under construction in caller-owned memory.
///
/// Blending uses linear light; output is premultiplied sRGB RGBA8. Interpolated
/// transfer tables avoid per-pixel powers, with byte quantization at each draw.
/// A failed draw may have modified pixels: discard that frame instead of
/// presenting it. Completed scenes remain valid and reusable after failures.
pub struct Frame<'r, 's, 'p> {
    pub(crate) renderer: &'r mut Renderer,
    pub(crate) surface: &'s mut Surface<'p>,
    /// Whole device pixels this frame may change.
    pub(crate) region: Rect,
    /// Open layers, innermost last; draws go to the innermost.
    pub(crate) layers: Vec<crate::layer::Open>,
}

impl Frame<'_, '_, '_> {
    /// Draws a retained scene with a logical-to-physical root transform.
    ///
    /// Each scene's scopes are local to this call. The root transform may include
    /// the layout offset and device scale, so moving a node does not require
    /// rebuilding its local drawing record. Device path coordinates beyond
    /// ±1,048,576 are rejected explicitly by this backend.
    pub fn draw(&mut self, scene: &Scene, transform: Affine) -> Result<(), RenderError> {
        self.draw_clipped(scene, transform, None)
    }

    /// Draws a retained scene intersected with an optional device-space rectangle.
    ///
    /// The clip is axis aligned and unaffected by `transform` or scene transforms.
    /// It intersects every scene clip and applies to shapes and text, only for
    /// this draw. `None` is identical to [`Self::draw`]. Zero extent excludes all
    /// pixels. Invalid, negative or out-of-range clip geometry returns Coordinates,
    /// including for empty scenes. A clip whose edges lie on whole device pixels
    /// only narrows the raster bounds; any other clip adds one mask layer to the
    /// draw's budget, plus the ordinary coverage mask when none was yet needed.
    pub fn draw_clipped(
        &mut self,
        scene: &Scene,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result<(), RenderError> {
        self.in_target(|frame, shift| {
            let transform = transform
                .then(shift)
                .map_err(|_| RenderError::Coordinates)?;
            let clip = clip.map(|rect| crate::layer::moved(rect, shift));
            frame.draw_target(scene, transform, clip)
        })
    }

    /// [`Self::draw_clipped`] into the current target.
    fn draw_target(
        &mut self,
        scene: &Scene,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result<(), RenderError> {
        // The frame's whole-pixel region bounds every draw, without a mask.
        let region = Bounds::aligned(self.region, self.surface).unwrap();
        // A clip on whole device pixels only narrows the raster bounds and
        // needs no surface-sized mask.
        if let Some(bounds) = clip.and_then(|rect| Bounds::aligned(rect, self.surface)) {
            let bounds = bounds.intersect(region);
            if scene.is_empty() {
                return Ok(());
            }
            self.renderer.prepare(scene, self.surface, false)?;
            let state = State {
                transform,
                clips: 0,
                bounds,
            };
            return self.commands(scene, state);
        }
        let clip = clip
            .map(|rect| {
                let shape = RoundedRect::new(rect, 0.0).map_err(|_| RenderError::Coordinates)?;
                if shape.is_empty() || scene.is_empty() {
                    path::validate_bounds([
                        rect.origin.x,
                        rect.origin.y,
                        rect.origin.x + rect.size.width,
                        rect.origin.y + rect.size.height,
                    ])?;
                    Ok(None)
                } else {
                    self.geometry(shape, None, Affine::IDENTITY).map(Some)
                }
            })
            .transpose()?;
        if scene.is_empty() {
            return Ok(());
        }
        self.renderer.prepare(scene, self.surface, clip.is_some())?;
        let mut state = State {
            transform,
            clips: 0,
            bounds: region,
        };
        if let Some(clip) = clip {
            if let Some(path) = clip {
                self.push_clip_path(path, &mut state);
            } else {
                state.clips = 1;
                state.bounds = Bounds::EMPTY;
            }
        }
        self.commands(scene, state)
    }

    fn commands(&mut self, scene: &Scene, mut state: State) -> Result<(), RenderError> {
        for command in scene.commands() {
            match *command {
                Command::PushTransform(local) => {
                    self.renderer.stack.push(state);
                    state.transform = local
                        .then(state.transform)
                        .map_err(|_| RenderError::Coordinates)?;
                }
                Command::PushClip(shape) => {
                    self.renderer.stack.push(state);
                    self.push_clip(shape, &mut state)?;
                }
                Command::Pop => state = self.renderer.stack.pop().unwrap(),
                Command::Fill { shape, color } => self.paint(shape, color, None, state)?,
                Command::Stroke {
                    shape,
                    color,
                    width,
                } => self.paint(shape, color, Some(width), state)?,
                Command::Image { image, rect } => {
                    self.paint_image(&scene.images()[image], rect, state)?
                }
                Command::FillPath { path, color } => {
                    self.paint_path(&scene.paths()[path], color, None, state)?
                }
                Command::StrokePath {
                    path,
                    color,
                    stroke,
                } => self.paint_path(&scene.paths()[path], color, Some(stroke), state)?,
                Command::FillGradient { shape, gradient } => {
                    self.paint_gradient(shape, &scene.gradients()[gradient], state)?
                }
                Command::Shadow { shape, color, blur } => {
                    self.paint_shadow(shape, color, blur, state)?
                }
                #[cfg(feature = "text")]
                Command::Glyphs(index) => self.paint_text(&scene.glyph_runs()[index], state)?,
                _ => return Err(RenderError::UnsupportedCommand),
            }
        }
        Ok(())
    }

    pub(crate) fn geometry(
        &mut self,
        shape: RoundedRect,
        width: Option<f32>,
        transform: Affine,
    ) -> Result<Path, RenderError> {
        let builder = std::mem::take(&mut self.renderer.path);
        path::build(builder, shape, width, transform)
    }

    fn push_clip(&mut self, shape: RoundedRect, state: &mut State) -> Result<(), RenderError> {
        if shape.is_empty() || state.bounds.is_empty() {
            state.clips += 1;
            state.bounds = Bounds::EMPTY;
            return Ok(());
        }
        let path = self.geometry(shape, None, state.transform)?;
        self.push_clip_path(path, state);
        Ok(())
    }

    fn push_clip_path(&mut self, path: Path, state: &mut State) {
        let previous = state.clips;
        state.clips += 1;
        state.bounds = state.bounds.intersect(Bounds::path(&path, self.surface));
        let (earlier, next) = self.renderer.masks.split_at_mut(state.clips);
        let mask = &mut next[0];
        rasterize(mask, &path, state.bounds, FillRule::EvenOdd);
        if previous > 0 {
            let parent = earlier[previous].data();
            let data = mask.data_mut();
            for row in state.bounds.rows(self.surface.width as usize) {
                for i in row {
                    data[i] = coverage_product(data[i], parent[i]);
                }
            }
        }
        self.renderer.path = path.clear();
    }

    /// Fills a device-space path, then recycles its storage for the next draw.
    pub(crate) fn fill_device(&mut self, path: Path, rule: FillRule, color: Color, state: State) {
        let bounds = state.bounds.intersect(Bounds::path(&path, self.surface));
        self.cover(&path, rule, color, state, bounds);
        self.renderer.path = path.clear();
    }

    /// Fills the part of a device-space path inside `bounds`.
    pub(crate) fn cover(
        &mut self,
        path: &Path,
        rule: FillRule,
        color: Color,
        state: State,
        bounds: Bounds,
    ) {
        self.cover_parts(path, rule, color, state, &[bounds]);
    }

    /// Fills the parts of a device-space path inside each of `parts`, which
    /// must contain every pixel the path covers, with one rasterization.
    pub(crate) fn cover_parts(
        &mut self,
        path: &Path,
        rule: FillRule,
        color: Color,
        state: State,
        parts: &[Bounds],
    ) {
        if parts.iter().all(|part| part.is_empty()) {
            return;
        }
        let (coverage, clips) = self.renderer.masks.split_at_mut(1);
        let mask = &mut coverage[0];
        // Only these pixels are read; others may keep an earlier draw.
        let width = mask.width() as usize;
        for row in parts.iter().flat_map(|part| part.rows(width)) {
            mask.data_mut()[row].fill(0);
        }
        mask.fill_path(path, rule, true, Transform::identity());
        let clip = state.clips.checked_sub(1).map(|index| clips[index].data());
        let paint = Solid::new(self.surface.color(color));
        let mask = mask.data();
        for row in parts.iter().flat_map(|part| part.rows(width)) {
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            paint.blend_span(pixels, row.start, |i| {
                clip.map_or(mask[i], |clip| coverage_product(mask[i], clip[i]))
            });
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) transform: Affine,
    pub(crate) clips: usize,
    pub(crate) bounds: Bounds,
}

pub(crate) fn rasterize(mask: &mut Mask, path: &Path, bounds: Bounds, rule: FillRule) {
    if bounds.is_empty() {
        return;
    }
    // Only samples in these bounds will be read. Other bytes may retain an old
    // draw; clearing the touched rows avoids a full-window memset per primitive.
    for row in bounds.rows(mask.width() as usize) {
        mask.data_mut()[row].fill(0);
    }
    mask.fill_path(path, rule, true, Transform::identity());
}

pub(crate) fn coverage_product(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}
