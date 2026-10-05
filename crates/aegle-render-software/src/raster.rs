use aegle_scene::{Affine, Command, RoundedRect, Scene};
use aegle_types::{Color, Rect};
use tiny_skia::{FillRule, IntSize, Mask, Path, PathBuilder, Transform};

use crate::{RenderError, Surface, blend::Solid, path};

/// Reusable CPU rendering state with an explicit coverage/clip mask budget.
///
/// The default budget is 2 MiB. Shape/clip draws reserve one byte per surface pixel
/// for coverage, plus one byte per pixel per nested clip. Text without clips
/// needs no surface-sized mask. Masks are retained for the
/// largest clip depth at the current surface size. Path/stack storage and the
/// rasterizer's temporary scanline buffers are not included in this budget.
/// Devices without a GPU can use this crate without any platform library.
pub struct Renderer {
    mask_budget: usize,
    pub(crate) masks: Vec<Mask>,
    #[cfg(feature = "text")]
    pub(crate) glyphs: aegle_glyph::GlyphCache,
    dimensions: (u32, u32),
    stack: Vec<State>,
    path: PathBuilder,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new(2 * 1024 * 1024)
    }
}

impl Renderer {
    /// Creates an allocation-free renderer with a mask limit in bytes.
    pub fn new(mask_budget: usize) -> Self {
        Self {
            mask_budget,
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
        if self.dimensions != (surface.width, surface.height) {
            self.masks.clear();
            self.dimensions = (surface.width, surface.height);
        }
        surface.clear(clear);
        Frame {
            renderer: self,
            surface,
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
        let needs_masks = depth > 0
            || scene
                .commands()
                .iter()
                .any(|command| matches!(command, Command::Fill { .. } | Command::Stroke { .. }));
        let count = if needs_masks { depth + 1 } else { 0 };
        let pixels = surface.data.len() / 4;
        let required = pixels.checked_mul(count).unwrap_or(usize::MAX);
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
        self.stack.clear();
        self.stack.reserve(scene.max_depth());
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
    /// including for empty scenes. An external clip adds one mask layer to the
    /// draw's budget, plus the ordinary coverage mask when none was yet needed.
    pub fn draw_clipped(
        &mut self,
        scene: &Scene,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result<(), RenderError> {
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
            bounds: Bounds::surface(self.surface.width, self.surface.height),
        };
        if let Some(clip) = clip {
            if let Some(path) = clip {
                self.push_clip_path(path, &mut state);
            } else {
                state.clips = 1;
                state.bounds = Bounds::EMPTY;
            }
        }
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
                #[cfg(feature = "text")]
                Command::Glyphs(index) => self.paint_text(&scene.glyph_runs()[index], state)?,
                _ => return Err(RenderError::UnsupportedCommand),
            }
        }
        Ok(())
    }

    fn geometry(
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
        rasterize(mask, &path, state.bounds);
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

    fn paint(
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
        let (coverage, clips) = self.renderer.masks.split_at_mut(1);
        let mask = &mut coverage[0];
        rasterize(mask, &path, bounds);
        let clip = state.clips.checked_sub(1).map(|index| clips[index].data());
        let paint = Solid::new(color);
        for row in bounds.rows(self.surface.width as usize) {
            let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
            for (i, pixel) in row.zip(pixels.chunks_exact_mut(4)) {
                let alpha = clip.map_or(mask.data()[i], |clip| {
                    coverage_product(mask.data()[i], clip[i])
                });
                paint.blend(pixel, alpha);
            }
        }
        self.renderer.path = path.clear();
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) transform: Affine,
    pub(crate) clips: usize,
    pub(crate) bounds: Bounds,
}

#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub(crate) left: usize,
    pub(crate) top: usize,
    pub(crate) right: usize,
    pub(crate) bottom: usize,
}

impl Bounds {
    const EMPTY: Self = Self {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    fn surface(width: u32, height: u32) -> Self {
        Self {
            right: width as usize,
            bottom: height as usize,
            ..Self::EMPTY
        }
    }
    fn path(path: &Path, surface: &Surface<'_>) -> Self {
        let b = path.bounds();
        Self {
            left: b.left().floor().clamp(0.0, surface.width as f32) as usize,
            top: b.top().floor().clamp(0.0, surface.height as f32) as usize,
            right: b.right().ceil().clamp(0.0, surface.width as f32) as usize,
            bottom: b.bottom().ceil().clamp(0.0, surface.height as f32) as usize,
        }
    }
    pub(crate) fn intersect(self, other: Self) -> Self {
        let result = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        if result.is_empty() {
            Self::EMPTY
        } else {
            result
        }
    }
    pub(crate) fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }
    pub(crate) fn rows(self, width: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
        (self.top..self.bottom).map(move |y| y * width + self.left..y * width + self.right)
    }
}

fn rasterize(mask: &mut Mask, path: &Path, bounds: Bounds) {
    if bounds.is_empty() {
        return;
    }
    // Only samples in these bounds will be read. Other bytes may retain an old
    // draw; clearing the touched rows avoids a full-window memset per primitive.
    for row in bounds.rows(mask.width() as usize) {
        mask.data_mut()[row].fill(0);
    }
    mask.fill_path(path, FillRule::EvenOdd, true, Transform::identity());
}

pub(crate) fn coverage_product(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}
