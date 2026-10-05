use alloc::vec::Vec;

use crate::{
    Affine, Color, Command, Image, MAX_SCOPE_DEPTH, Path, Rect, RoundedRect, SceneError, Stroke,
};

/// Immutable validated drawing commands with no renderer or tree ownership.
#[derive(Debug, Default)]
pub struct Scene {
    commands: Vec<Command>,
    #[cfg(feature = "text")]
    glyph_runs: Vec<crate::GlyphRun>,
    images: Vec<Image>,
    paths: Vec<Path>,
    max_depth: usize,
    max_clip_depth: usize,
}

impl Scene {
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
            + self.paths.capacity() * core::mem::size_of::<Path>();
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
/// fixed inline stack (about 1.6 KiB), avoiding a second heap allocation while
/// recording. Only commands and their resources remain in the completed scene.
#[derive(Debug)]
pub struct SceneBuilder {
    scene: Scene,
    saved: [Affine; MAX_SCOPE_DEPTH],
    clips: [bool; MAX_SCOPE_DEPTH],
    transform: Affine,
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
            transform: Affine::IDENTITY,
            depth: 0,
            clip_depth: 0,
        }
    }

    /// Discards the record and open scopes, retaining command-buffer capacity.
    pub fn clear(&mut self) {
        self.scene.commands.clear();
        #[cfg(feature = "text")]
        self.scene.glyph_runs.clear();
        self.scene.images.clear();
        self.scene.paths.clear();
        self.scene.max_depth = 0;
        self.scene.max_clip_depth = 0;
        self.transform = Affine::IDENTITY;
        self.depth = 0;
        self.clip_depth = 0;
    }

    /// Records a solid fill. Empty shapes produce no command.
    pub fn fill(&mut self, shape: RoundedRect, color: Color) -> Result<&mut Self, SceneError> {
        if !shape.is_empty() {
            self.transform.validate_shape(shape, 0.0)?;
            self.scene.commands.push(Command::Fill { shape, color });
        }
        Ok(self)
    }

    /// Records `image` stretched over `rect`. Empty rectangles produce no command.
    pub fn image(&mut self, image: &Image, rect: Rect) -> Result<&mut Self, SceneError> {
        let shape = RoundedRect::new(rect, 0.0)?;
        if !shape.is_empty() {
            self.transform.validate_shape(shape, 0.0)?;
            let image_index = self.scene.images.len();
            self.scene.images.push(image.clone());
            self.scene.commands.push(Command::Image {
                image: image_index,
                rect,
            });
        }
        Ok(self)
    }

    /// Fills `path` with its fill rule. Paths without segments produce no command.
    pub fn fill_path(&mut self, path: &Path, color: Color) -> Result<&mut Self, SceneError> {
        if !path.is_empty() {
            self.transform
                .validate_shape(RoundedRect::new(path.bounds(), 0.0)?, 0.0)?;
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
        if !run.glyphs().is_empty() {
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
        self.push(Command::PushClip(shape), self.transform)
    }

    fn push(&mut self, command: Command, transform: Affine) -> Result<&mut Self, SceneError> {
        if self.depth == MAX_SCOPE_DEPTH {
            return Err(SceneError::ScopeLimit);
        }
        self.saved[self.depth] = self.transform;
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
