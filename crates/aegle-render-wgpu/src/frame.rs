//! Public frame handle and the scene walker that fills draw records.
use aegle_scene::{Affine, Command, MAX_SCOPE_DEPTH, RoundedRect, Scene};
use aegle_types::Rect;

use crate::{
    Error, Renderer, Result,
    records::{NO_CLIP, State},
    renderer::MAX_PRIMITIVES,
};

pub(crate) enum Presentation {
    Offscreen,
    #[cfg(feature = "window")]
    Surface(wgpu::SurfaceTexture),
}

/// One complete frame. Draw calls are painted in order; [`Frame::finish`]
/// submits the final GPU work. Dropping a frame never presents it.
pub struct Frame<'a> {
    renderer: &'a mut Renderer,
    presentation: Presentation,
    failed: bool,
}

impl<'a> Frame<'a> {
    pub(crate) fn new(renderer: &'a mut Renderer, presentation: Presentation) -> Self {
        Self {
            renderer,
            presentation,
            failed: false,
        }
    }

    /// Draws `scene` under `transform`. Any error poisons the frame, which then
    /// refuses to finish, so a partial scene is never presented.
    pub fn draw(&mut self, scene: &Scene, transform: Affine) -> Result {
        self.draw_clipped(scene, transform, None)
    }

    /// Like [`Self::draw`] with an extra clip in device coordinates. It is not
    /// transformed by `transform` and does not affect later draws.
    pub fn draw_clipped(&mut self, scene: &Scene, transform: Affine, clip: Option<Rect>) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        let result = self.renderer.walk(scene, transform, clip);
        self.failed = result.is_err();
        result
    }

    /// Submits the frame; a window frame is then presented. Offscreen output is
    /// read back explicitly with [`Renderer::read_pixels`].
    pub fn finish(self) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        match self.presentation {
            Presentation::Offscreen => self.renderer.finish(None),
            #[cfg(feature = "window")]
            Presentation::Surface(texture) => {
                self.renderer.finish(Some(&texture.texture))?;
                self.renderer.gpu.queue.present(texture);
                Ok(())
            }
        }
    }
}

impl Renderer {
    /// Splits the frame once a submission's worth of records has accumulated.
    pub(crate) fn flush_full(&mut self) -> Result {
        if self.rec.primitives.len() >= MAX_PRIMITIVES {
            self.flush(None)?;
        }
        Ok(())
    }

    fn walk(&mut self, scene: &Scene, transform: Affine, clip: Option<Rect>) -> Result {
        if scene.max_clip_depth() + usize::from(clip.is_some()) > 8 {
            return Err(Error::ClipDepth);
        }
        let [width, height] = self.size;
        let viewport = [width as f32, height as f32];
        let mut state = State {
            transform,
            clip: NO_CLIP,
            bounds: [0.0, 0.0, viewport[0], viewport[1]],
        };
        if let Some(rect) = clip {
            let shape = RoundedRect::new(rect, 0.0)?;
            self.rec.push_clip(&mut state, shape, Affine::IDENTITY)?;
        }
        // Scene scopes have a validated maximum; no per-draw heap scratch.
        let mut saved = [state; MAX_SCOPE_DEPTH];
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
                    self.rec.push_clip(&mut state, shape, transform)?;
                }
                Command::Pop => {
                    depth -= 1;
                    state = saved[depth];
                }
                Command::Fill { shape, color } => {
                    self.rec.shape(state, shape, color, -1.0, viewport)?;
                }
                Command::Stroke {
                    shape,
                    color,
                    width,
                } => self.rec.shape(state, shape, color, width, viewport)?,
                #[cfg(feature = "text")]
                Command::Glyphs(index) => {
                    self.glyphs(&scene.glyph_runs()[index], state)?;
                }
                _ => return Err(Error::UnsupportedCommand),
            }
            self.flush_full()?;
        }
        Ok(())
    }
}
