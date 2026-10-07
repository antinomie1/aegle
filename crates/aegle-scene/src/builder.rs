use alloc::vec::Vec;

use crate::{
    Affine, Color, Command, Gradient, Image, MAX_SCOPE_DEPTH, Path, Rect, RoundedRect, SceneError,
    Stroke, TextureId,
};

/// Immutable validated drawing commands with no renderer or tree ownership.
#[derive(Debug, Default)]
pub struct Scene {
    commands: Vec<Command>,
    #[cfg(feature = "text")]
    glyph_runs: Vec<crate::GlyphRun>,
    images: Vec<Image>,
    paths: Vec<Path>,
    gradients: Vec<Gradient>,
    max_depth: usize,
    max_clip_depth: usize,
    bounds: Option<Rect>,
}

impl Scene {
    /// A rectangle, in the scene's coordinates, containing every pixel its
    /// commands may draw, within their clips; `None` when nothing draws.
    /// Glyphs count a conservative box of the run's em squares.
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }

    /// Returns drawing commands in their compositing order.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Number of recorded operations, including scope pushes and pops.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the record contains no operations.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Maximum combined transform and clip nesting used by this scene.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Maximum nested clips, excluding transform scopes, for backend budgeting.
    pub fn max_clip_depth(&self) -> usize {
        self.max_clip_depth
    }

    /// Reserved command, resource-handle and glyph-run storage, including
    /// glyph/variation arrays.
    ///
    /// Excludes the inline scene header, allocator overhead, shared font bytes and
    /// shared image/path storage.
    pub fn allocated_bytes(&self) -> usize {
        let bytes = self.commands.capacity() * core::mem::size_of::<Command>()
            + self.images.capacity() * core::mem::size_of::<Image>()
            + self.paths.capacity() * core::mem::size_of::<Path>()
            + self.gradients.capacity() * core::mem::size_of::<Gradient>();
        #[cfg(feature = "text")]
        let bytes = bytes
            + self.glyph_runs.capacity() * core::mem::size_of::<crate::GlyphRun>()
            + self
                .glyph_runs
                .iter()
                .map(crate::GlyphRun::allocated_bytes)
                .sum::<usize>();
        bytes
    }

    /// Shared images addressed by [`Command::Image`].
    pub fn images(&self) -> &[Image] {
        &self.images
    }

    /// Shared outlines addressed by [`Command::FillPath`] and [`Command::StrokePath`].
    pub fn paths(&self) -> &[Path] {
        &self.paths
    }

    /// Gradients addressed by [`Command::FillGradient`].
    pub fn gradients(&self) -> &[Gradient] {
        &self.gradients
    }

    /// Positioned glyph resources addressed by [`Command::Glyphs`].
    #[cfg(feature = "text")]
    pub fn glyph_runs(&self) -> &[crate::GlyphRun] {
        &self.glyph_runs
    }

    /// Reopens this balanced scene for appending, preserving its command storage.
    ///
    /// Call [`SceneBuilder::clear`] to replace the record without reallocating.
    pub fn into_builder(self) -> SceneBuilder {
        SceneBuilder {
            scene: self,
            ..SceneBuilder::new()
        }
    }
}

/// Records drawing operations, validating geometry and scope state at insertion.
///
/// A rejected operation leaves the builder unchanged. Transform state uses a
/// fixed inline stack (about 2.9 KiB, with each scope's clip box), avoiding a second heap allocation while
/// recording. Only commands and their resources remain in the completed scene.
#[derive(Debug)]
pub struct SceneBuilder {
    scene: Scene,
    saved: [Affine; MAX_SCOPE_DEPTH],
    clips: [bool; MAX_SCOPE_DEPTH],
    saved_clip: [Option<Rect>; MAX_SCOPE_DEPTH],
    transform: Affine,
    /// The current clip's bounding box in scene coordinates.
    clip: Option<Rect>,
    depth: usize,
    clip_depth: usize,
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneBuilder {
    /// Creates an empty record without allocating until the first command.
    pub fn new() -> Self {
        Self {
            scene: Scene::default(),
            saved: [Affine::IDENTITY; MAX_SCOPE_DEPTH],
            clips: [false; MAX_SCOPE_DEPTH],
            saved_clip: [None; MAX_SCOPE_DEPTH],
            transform: Affine::IDENTITY,
            clip: None,
            depth: 0,
            clip_depth: 0,
        }
    }

    /// Grows the scene bounds by `rect` grown by `outset`, in the current
    /// transform and within the current clip.
    fn cover(&mut self, rect: Rect, outset: f32) {
        let drawn = self.transform.bounds(rect, outset);
        let drawn = match self.clip {
            Some(clip) => match drawn.intersection(clip) {
                Some(drawn) => drawn,
                None => return,
            },
            None => drawn,
        };
        self.scene.bounds = Some(self.scene.bounds.map_or(drawn, |b| b.union(drawn)));
    }

    /// Discards the record and open scopes, retaining command-buffer capacity.
    pub fn clear(&mut self) {
        self.scene.commands.clear();
        #[cfg(feature = "text")]
        self.scene.glyph_runs.clear();
        self.scene.images.clear();
        self.scene.paths.clear();
        self.scene.gradients.clear();
        self.scene.max_depth = 0;
        self.scene.max_clip_depth = 0;
        self.scene.bounds = None;
        self.transform = Affine::IDENTITY;
        self.clip = None;
        self.depth = 0;
        self.clip_depth = 0;
    }

    /// Records a solid fill. Empty shapes produce no command.
    pub fn fill(&mut self, shape: RoundedRect, color: Color) -> Result<&mut Self, SceneError> {
        if !shape.is_empty() {
            self.transform.validate_shape(shape, 0.0)?;
            self.cover(shape.rect(), 0.0);
            self.scene.commands.push(Command::Fill { shape, color });
        }
        Ok(self)
    }

    /// Fills `shape` with `gradient`, whose geometry shares the shape's local
    /// coordinates. Empty shapes produce no command.
    pub fn fill_gradient(
        &mut self,
        shape: RoundedRect,
        gradient: &Gradient,
    ) -> Result<&mut Self, SceneError> {
        if !shape.is_empty() {
            self.transform.validate_shape(shape, 0.0)?;
            self.cover(shape.rect(), 0.0);
            self.scene.gradients.push(gradient.clone());
            let gradient = self.scene.gradients.len() - 1;
            self.scene
                .commands
                .push(Command::FillGradient { shape, gradient });
        }
        Ok(self)
    }

    /// Records the soft shadow of `shape`; see [`Command::Shadow`]. A zero
    /// `blur` records an ordinary fill; empty shapes produce no command.
    pub fn shadow(
        &mut self,
        shape: RoundedRect,
        color: Color,
        blur: f32,
    ) -> Result<&mut Self, SceneError> {
        if !blur.is_finite() {
            return Err(SceneError::NonFinite);
        }
        if blur < 0.0 {
            return Err(SceneError::NegativeExtent);
        }
        if blur == 0.0 {
            return self.fill(shape, color);
        }
        if !shape.is_empty() {
            self.transform.validate_shape(shape, blur * 3.0)?;
            self.cover(shape.rect(), blur * 3.0);
            self.scene
                .commands
                .push(Command::Shadow { shape, color, blur });
        }
        Ok(self)
    }

    /// Records `image` stretched over `rect`. Empty rectangles produce no command.
    pub fn image(&mut self, image: &Image, rect: Rect) -> Result<&mut Self, SceneError> {
        let shape = RoundedRect::new(rect, 0.0)?;
        if !shape.is_empty() {
            self.transform.validate_shape(shape, 0.0)?;
            self.cover(rect, 0.0);
            let image_index = self.scene.images.len();
            self.scene.images.push(image.clone());
            self.scene.commands.push(Command::Image {
                image: image_index,
                rect,
            });
        }
        Ok(self)
    }

    /// Draws a registered application texture stretched over `rect`. Empty
    /// rectangles produce no command; see [`Command::Texture`]. Only GPU
    /// renderers draw textures: in a native app, a texture can be registered
    /// exactly when `App::wgpu` or `App::vulkan` returns a device.
    pub fn texture(&mut self, texture: TextureId, rect: Rect) -> Result<&mut Self, SceneError> {
        let shape = RoundedRect::new(rect, 0.0)?;
        if !shape.is_empty() {
            self.transform.validate_shape(shape, 0.0)?;
            self.cover(rect, 0.0);
            self.scene.commands.push(Command::Texture { texture, rect });
        }
        Ok(self)
    }

    /// Fills `path` with its fill rule. Paths without segments produce no command.
    pub fn fill_path(&mut self, path: &Path, color: Color) -> Result<&mut Self, SceneError> {
        if !path.is_empty() {
            self.transform
                .validate_shape(RoundedRect::new(path.bounds(), 0.0)?, 0.0)?;
            self.cover(path.bounds(), 0.0);
            let index = self.push_path(path);
            self.scene
                .commands
                .push(Command::FillPath { path: index, color });
        }
        Ok(self)
    }

    /// Strokes `path`. Paths without segments and zero widths produce no command.
    pub fn stroke_path(
        &mut self,
        path: &Path,
        color: Color,
        stroke: Stroke,
    ) -> Result<&mut Self, SceneError> {
        if !stroke.width.is_finite() {
            return Err(SceneError::NonFinite);
        }
        if stroke.width < 0.0 {
            return Err(SceneError::NegativeExtent);
        }
        if !path.is_empty() && stroke.width > 0.0 {
            // Miter joins reach at most twice the width (limit 4) from the outline.
            let shape = RoundedRect::new(path.bounds(), 0.0)?;
            self.transform.validate_shape(shape, stroke.width * 2.0)?;
            self.cover(path.bounds(), stroke.width * 2.0);
            let index = self.push_path(path);
            self.scene.commands.push(Command::StrokePath {
                path: index,
                color,
                stroke,
            });
        }
        Ok(self)
    }

    fn push_path(&mut self, path: &Path) -> usize {
        self.scene.paths.push(path.clone());
        self.scene.paths.len() - 1
    }

    /// Records positioned glyphs, retaining a shared font handle, never bitmaps.
    /// The rasterizer owns any glyph cache. Empty runs produce no command.
    #[cfg(feature = "text")]
    pub fn glyphs(&mut self, run: crate::GlyphRun) -> Result<&mut Self, SceneError> {
        for glyph in run.glyphs() {
            let p = self.transform.map_point(glyph.position);
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(SceneError::CoordinateRange);
            }
        }
        if let Some(first) = run.glyphs().first() {
            // Pen positions sit on the baseline; an em square above and
            // below, and wider for overhang and synthetic styles, contains
            // every glyph's ink.
            let (mut low, mut high) = (first.position, first.position);
            for glyph in run.glyphs() {
                (low.x, low.y) = (low.x.min(glyph.position.x), low.y.min(glyph.position.y));
                (high.x, high.y) = (high.x.max(glyph.position.x), high.y.max(glyph.position.y));
            }
            let size = run.size();
            let pens = Rect::new(
                low.x,
                low.y - size,
                high.x - low.x + size,
                high.y - low.y + size,
            );
            self.cover(pens, size);
            let index = self.scene.glyph_runs.len();
            self.scene.glyph_runs.push(run);
            self.scene.commands.push(Command::Glyphs(index));
        }
        Ok(self)
    }

    /// Records a centered stroke. Empty shapes and zero widths produce no command.
    ///
    /// The width is measured in local coordinates and scales with the transform.
    pub fn stroke(
        &mut self,
        shape: RoundedRect,
        color: Color,
        width: f32,
    ) -> Result<&mut Self, SceneError> {
        if !width.is_finite() {
            return Err(SceneError::NonFinite);
        }
        if width < 0.0 {
            return Err(SceneError::NegativeExtent);
        }
        if !shape.is_empty() && width > 0.0 {
            self.transform.validate_shape(shape, width * 0.5)?;
            self.cover(shape.rect(), width * 0.5);
            self.scene.commands.push(Command::Stroke {
                shape,
                color,
                width,
            });
        }
        Ok(self)
    }

    /// Opens a local-to-parent transform scope, validating its composition.
    pub fn push_transform(&mut self, local: Affine) -> Result<&mut Self, SceneError> {
        let composed = local.then(self.transform)?;
        self.push(Command::PushTransform(local), composed)
    }

    /// Opens a clip scope; the clip captures the current transform.
    ///
    /// An empty shape clips all subsequent drawing until its matching pop.
    pub fn push_clip(&mut self, shape: RoundedRect) -> Result<&mut Self, SceneError> {
        self.transform.validate_shape(shape, 0.0)?;
        let clip = self.transform.bounds(shape.rect(), 0.0);
        let clip = match self.clip {
            Some(outer) => outer
                .intersection(clip)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            None => clip,
        };
        self.push(Command::PushClip(shape), self.transform)?;
        self.clip = Some(clip);
        Ok(self)
    }

    fn push(&mut self, command: Command, transform: Affine) -> Result<&mut Self, SceneError> {
        if self.depth == MAX_SCOPE_DEPTH {
            return Err(SceneError::ScopeLimit);
        }
        self.saved[self.depth] = self.transform;
        self.saved_clip[self.depth] = self.clip;
        self.clips[self.depth] = matches!(command, Command::PushClip(_));
        self.clip_depth += usize::from(self.clips[self.depth]);
        self.depth += 1;
        self.transform = transform;
        self.scene.max_depth = self.scene.max_depth.max(self.depth);
        self.scene.max_clip_depth = self.scene.max_clip_depth.max(self.clip_depth);
        self.scene.commands.push(command);
        Ok(self)
    }

    /// Closes the most recent transform or clip scope.
    pub fn pop(&mut self) -> Result<&mut Self, SceneError> {
        if self.depth == 0 {
            return Err(SceneError::UnexpectedPop);
        }
        self.depth -= 1;
        self.clip_depth -= usize::from(self.clips[self.depth]);
        self.transform = self.saved[self.depth];
        self.clip = self.saved_clip[self.depth];
        self.scene.commands.push(Command::Pop);
        Ok(self)
    }

    /// Finishes recording. Every pushed scope must have a matching pop.
    pub fn finish(self) -> Result<Scene, SceneError> {
        if self.depth != 0 {
            return Err(SceneError::UnclosedScope);
        }
        Ok(self.scene)
    }
}
