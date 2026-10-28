use aegle_scene::{Affine, Command, RoundedRect, Scene};
use aegle_types::{Color, Rect};

use crate::blend::Solid;
use crate::bounds::Bounds;
use crate::coverage::{Area, Mask};
use crate::outline;
use crate::{Rasterizer, RenderError, Surface};

pub(crate) use aegle_scene::FillRule;

/// Reusable CPU compositing state over a [`Rasterizer`], with an optional mask budget.
///
/// Shape and clip draws reserve one byte per surface pixel for coverage, plus one
/// byte per pixel per nested clip; clips nest at most eight deep, as on the GPU
/// backends. Text without clips needs no surface-sized mask. The default budget
/// is unlimited, since the surface size is the window's; a byte limit caps masks
/// on small devices. Masks are retained for the largest clip depth at the current
/// surface size. Mask rows are padded to a four-byte multiple so a rasterizer can
/// address them directly, and the budget counts those padded bytes.
///
/// Path and scanline storage belongs to the rasterizer and is not part of this
/// budget. Devices without a GPU can use this crate without any platform library.
pub struct Renderer<R: Rasterizer> {
    mask_budget: usize,
    /// Byte limit of layer images and blur scratch.
    pub(crate) effect_budget: usize,
    /// Pixel storage of finished layers, reused by later ones.
    pub(crate) spare: Vec<Vec<u8>>,
    /// Blur scratch: two premultiplied linear buffers of the sampled area.
    pub(crate) blur: [Vec<[f32; 4]>; 2],
    /// Backdrop blurs skipped for exceeding the effect budget.
    pub(crate) skipped_blurs: u64,
    pub(crate) masks: Vec<R::Mask>,
    #[cfg(feature = "text")]
    pub(crate) glyphs: aegle_glyph::GlyphCache,
    dimensions: (u32, u32),
    stack: Vec<State>,
    pub(crate) raster: R,
}

impl<R: Rasterizer + Default> Default for Renderer<R> {
    fn default() -> Self {
        Self::new(usize::MAX)
    }
}

impl<R: Rasterizer + Default> Renderer<R> {
    /// Creates an allocation-free renderer with a mask limit in bytes.
    pub fn new(mask_budget: usize) -> Self {
        Self::with_rasterizer(R::default(), mask_budget)
    }
}

impl<R: Rasterizer> Renderer<R> {
    /// Creates an allocation-free renderer around an explicit rasterizer.
    pub fn with_rasterizer(raster: R, mask_budget: usize) -> Self {
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
            raster,
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
    ) -> Frame<'r, 's, 'p, R> {
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
    ) -> Frame<'r, 's, 'p, R> {
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
        self.masks.iter().map(Mask::allocated_bytes).sum()
    }

    /// The rasterizer this renderer composites over.
    pub fn rasterizer(&self) -> &R {
        &self.raster
    }

    /// Mutable access to the rasterizer, for its own configuration.
    pub fn rasterizer_mut(&mut self) -> &mut R {
        &mut self.raster
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

    /// Releases reusable masks and traversal storage when memory is needed.
    /// The next drawing operation allocates again. Transfer LUTs remain shared.
    /// Glyph scaling scratch is released, but completed glyph images remain;
    /// clear them separately through `glyph_cache_mut().clear()` with `text`.
    pub fn release_scratch(&mut self) {
        self.masks = Vec::new();
        self.spare = Vec::new();
        self.blur = [Vec::new(), Vec::new()];
        self.stack = Vec::new();
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
        let (width, height) = (surface.width, surface.height);
        // Checked before allocating, so an over-budget draw fails without
        // reserving memory it cannot use.
        let required = R::Mask::bytes_for(width, height).saturating_mul(count);
        if required > self.mask_budget {
            return Err(RenderError::MaskBudget {
                required,
                limit: self.mask_budget,
            });
        }
        while self.masks.len() < count {
            self.masks.push(R::Mask::new(width, height)?);
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
pub struct Frame<'r, 's, 'p, R: Rasterizer> {
    pub(crate) renderer: &'r mut Renderer<R>,
    pub(crate) surface: &'s mut Surface<'p>,
    /// Whole device pixels this frame may change.
    pub(crate) region: Rect,
    /// Open layers, innermost last; draws go to the innermost.
    pub(crate) layers: Vec<crate::layer::Open<R::Mask>>,
}

impl<R: Rasterizer> Frame<'_, '_, '_, R> {
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
    /// pixels. Nonfinite or negative clip geometry panics like scene geometry;
    /// a clip beyond the device coordinate range returns Coordinates, including
    /// for empty scenes. A clip whose edges lie on whole device pixels
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
        let region = Bounds::aligned(self.region, self.surface).unwrap_or(Bounds::EMPTY);
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
                let shape = RoundedRect::new(rect, 0.0);
                if shape.is_empty() || scene.is_empty() {
                    outline::validate_bounds([
                        rect.origin.x,
                        rect.origin.y,
                        rect.origin.x + rect.size.width,
                        rect.origin.y + rect.size.height,
                    ])?;
                    Ok(None)
                } else {
                    self.shape(shape, None, Affine::IDENTITY).map(Some)
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
                self.push_clip_path(path, &mut state)?;
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
                Command::Pop => state = self.renderer.stack.pop().expect("balanced scopes"),
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

    /// Builds a device-space outline of a shape through the rasterizer.
    pub(crate) fn shape(
        &mut self,
        shape: RoundedRect,
        width: Option<f32>,
        transform: Affine,
    ) -> Result<R::Path, RenderError> {
        self.renderer.raster.shape(shape, width, transform)
    }

    /// Builds a device-space outline of a scene path through the rasterizer.
    pub(crate) fn scene_path(
        &mut self,
        path: &aegle_scene::Path,
        stroke: Option<aegle_scene::Stroke>,
        transform: Affine,
    ) -> Result<R::Path, RenderError> {
        self.renderer.raster.path(path, stroke, transform)
    }

    /// Device pixels an outline may touch, clamped to the surface.
    pub(crate) fn path_bounds(&self, path: &R::Path) -> Bounds {
        Bounds::of(self.renderer.raster.bounds(path), self.surface)
    }

    fn push_clip(&mut self, shape: RoundedRect, state: &mut State) -> Result<(), RenderError> {
        if shape.is_empty() || state.bounds.is_empty() {
            state.clips += 1;
            state.bounds = Bounds::EMPTY;
            return Ok(());
        }
        let path = self.shape(shape, None, state.transform)?;
        self.push_clip_path(path, state)
    }

    fn push_clip_path(&mut self, path: R::Path, state: &mut State) -> Result<(), RenderError> {
        let previous = state.clips;
        state.clips += 1;
        state.bounds = state.bounds.intersect(self.path_bounds(&path));
        let bounds = state.bounds;
        if !bounds.is_empty() {
            let (earlier, next) = self.renderer.masks.split_at_mut(state.clips);
            let mask = &mut next[0];
            fill_mask(
                &mut self.renderer.raster,
                mask,
                &path,
                FillRule::EvenOdd,
                bounds,
            )?;
            if previous > 0 {
                let parent = &earlier[previous];
                for y in bounds.top..bounds.bottom {
                    let parent = parent.row(y);
                    let row = mask.row_mut(y);
                    for x in bounds.left..bounds.right {
                        row[x] = coverage_product(row[x], parent[x]);
                    }
                }
            }
        }
        self.renderer.raster.release(path);
        Ok(())
    }

    /// Fills a device-space outline, then recycles its storage for the next draw.
    pub(crate) fn fill_device(
        &mut self,
        path: R::Path,
        rule: FillRule,
        color: Color,
        state: State,
    ) -> Result<(), RenderError> {
        let bounds = state.bounds.intersect(self.path_bounds(&path));
        let result = self.cover(&path, rule, color, state, bounds);
        self.renderer.raster.release(path);
        result
    }

    /// Fills the part of a device-space outline inside `bounds`.
    pub(crate) fn cover(
        &mut self,
        path: &R::Path,
        rule: FillRule,
        color: Color,
        state: State,
        bounds: Bounds,
    ) -> Result<(), RenderError> {
        self.cover_parts(path, rule, color, state, &[bounds])
    }

    /// Fills the parts of a device-space outline inside each of `parts`, which
    /// must contain every pixel the outline covers, with one rasterization.
    pub(crate) fn cover_parts(
        &mut self,
        path: &R::Path,
        rule: FillRule,
        color: Color,
        state: State,
        parts: &[Bounds],
    ) -> Result<(), RenderError> {
        if parts.iter().all(|part| part.is_empty()) {
            return Ok(());
        }
        let (coverage, clips) = self.renderer.masks.split_at_mut(1);
        let mask = &mut coverage[0];
        // Parts are bands of one rectangle, so filling their union is the same
        // work and leaves rows outside the parts untouched.
        let area = parts
            .iter()
            .copied()
            .filter(|part| !part.is_empty())
            .fold(Bounds::EMPTY, union);
        fill_mask(&mut self.renderer.raster, mask, path, rule, area)?;
        let paint = Solid::new(self.surface.color(color));
        let width = self.surface.width as usize;
        let clip = state.clips.checked_sub(1).map(|index| &clips[index]);
        for row in parts.iter().flat_map(|part| part.rows(width)) {
            let y = row.start / width;
            let (pixels, mask_row) = (
                &mut self.surface.data[row.start * 4..row.end * 4],
                mask.row(y),
            );
            let clip_row = clip.map(|clip| clip.row(y));
            paint.blend_span(pixels, row.start, |i| {
                let x = i % width;
                clip_row.map_or(mask_row[x], |clip| coverage_product(mask_row[x], clip[x]))
            });
        }
        Ok(())
    }
}

/// Fills `path` into the whole-pixel area `bounds` of `mask`.
pub(crate) fn fill_mask<R: Rasterizer>(
    raster: &mut R,
    mask: &mut R::Mask,
    path: &R::Path,
    rule: FillRule,
    bounds: Bounds,
) -> Result<(), RenderError> {
    if bounds.is_empty() {
        return Ok(());
    }
    let area = Area {
        left: bounds.left,
        top: bounds.top,
        right: bounds.right,
        bottom: bounds.bottom,
    };
    // Only these pixels are read afterwards; others may keep an earlier draw.
    mask.clear(area);
    raster.fill(path, rule, area, mask)
}

pub(crate) fn union(a: Bounds, b: Bounds) -> Bounds {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    Bounds {
        left: a.left.min(b.left),
        top: a.top.min(b.top),
        right: a.right.max(b.right),
        bottom: a.bottom.max(b.bottom),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) transform: Affine,
    pub(crate) clips: usize,
    pub(crate) bounds: Bounds,
}

pub(crate) fn coverage_product(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}
