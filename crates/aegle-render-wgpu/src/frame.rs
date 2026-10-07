//! Public frame handle and the scene walker that fills draw records.
use aegle_gpu::{Step, Walker};
use aegle_scene::{Affine, Scene};
use aegle_types::Rect;

use aegle_gpu::MAX_PRIMITIVES;

use crate::{Error, Renderer, Result};

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
        let mut walker = Walker::new(
            scene,
            transform,
            clip,
            self.size,
            self.viewport,
            &mut self.rec,
        )?;
        loop {
            match walker.step(&mut self.rec)? {
                Step::Done => return Ok(()),
                Step::Recorded => {}
                #[cfg(feature = "text")]
                Step::Command(command, state) => {
                    use aegle_scene::Command;
                    match command {
                        Command::Glyphs(index) => self.glyphs(&scene.glyph_runs()[index], state)?,
                        Command::Image { image, rect } => {
                            self.image(&scene.images()[image], rect, state)?;
                        }
                        Command::FillPath { path, color } => {
                            self.path(&scene.paths()[path], color, None, state)?;
                        }
                        Command::StrokePath {
                            path,
                            color,
                            stroke,
                        } => self.path(&scene.paths()[path], color, Some(stroke), state)?,
                        Command::Texture { texture, rect } => self.texture(texture, rect, state)?,
                        _ => return Err(Error::UnsupportedCommand),
                    }
                }
                #[cfg(not(feature = "text"))]
                Step::Command(..) => return Err(Error::UnsupportedCommand),
            }
            self.flush_full()?;
        }
    }
}
