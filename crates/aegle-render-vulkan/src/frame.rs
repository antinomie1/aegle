//! Recording a frame, split into several submissions when it outgrows one.

use aegle_gpu::{MAX_PRIMITIVES, Step, Walker, viewport};
use aegle_scene::{Affine, Color, Rect, Scene};

use crate::{Error, Renderer, Result};

/// A frame recorded on the CPU, borrowing its renderer exclusively.
///
/// Like the wgpu renderer, a frame is submitted in parts once it holds
/// [`MAX_PRIMITIVES`] records, or when the glyph atlas, the upload buffer or the
/// application texture slots fill: the recorded part is drawn, the frame waits
/// for it, and recording continues from the glyph or command that did not fit.
/// Only content that does not fit an empty atlas fails. Dropping a frame
/// discards what was not submitted and presents nothing; a failed draw poisons
/// the frame so partially recorded content cannot accidentally be presented.
pub struct Frame<'a> {
    pub(crate) renderer: &'a mut Renderer,
    pub(crate) clear: Color,
    pub(crate) failed: bool,
}

impl Frame<'_> {
    /// Actual physical extent of this frame, including native surface constraints.
    pub fn extent(&self) -> [u32; 2] {
        let target = self.renderer.target.as_ref().unwrap();
        [target.width, target.height]
    }

    /// Appends a retained scene with a logical-to-device transform.
    pub fn draw(&mut self, scene: &Scene, transform: Affine) -> Result {
        self.draw_clipped(scene, transform, None)
    }

    /// Appends a scene intersected with an optional device-space clip. The clip
    /// is not transformed again and consumes one of the eight available clip layers.
    pub fn draw_clipped(&mut self, scene: &Scene, transform: Affine, clip: Option<Rect>) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        let (renderer, clear) = (&mut *self.renderer, self.clear);
        let result = (|| -> Result {
            #[cfg(feature = "text")]
            let (transform, clip, extent) = {
                let (transform, clip, extent, hidden) = renderer.placement(transform, clip)?;
                if hidden {
                    return Ok(());
                }
                (transform, clip, extent)
            };
            #[cfg(not(feature = "text"))]
            let extent = {
                let target = renderer.target.as_ref().unwrap();
                [target.width, target.height]
            };
            let view = viewport(extent[0], extent[1], false);
            let mut walker = Walker::new(
                scene,
                transform,
                clip,
                extent,
                view,
                &mut renderer.recording,
            )?;
            loop {
                match walker.step(&mut renderer.recording)? {
                    Step::Done => return Ok(()),
                    Step::Recorded => {}
                    #[cfg(feature = "text")]
                    Step::Command(command, state) => loop {
                        match renderer.record(scene, command, state, view) {
                            // Drawing what the frame holds frees its atlas pages,
                            // uploads and texture slots; with nothing recorded,
                            // the content cannot fit at all.
                            Err(
                                Error::AtlasFull | Error::TooManyTextures | Error::Budget { .. },
                            ) if !renderer.recording.primitives.is_empty() => {
                                renderer.flush_current(clear)?;
                            }
                            result => break result?,
                        }
                    },
                    #[cfg(not(feature = "text"))]
                    Step::Command(..) => return Err(Error::UnsupportedCommand),
                }
                if renderer.recording.primitives.len() >= MAX_PRIMITIVES {
                    renderer.flush_current(clear)?;
                }
            }
        })();
        if result.is_err() {
            self.failed = true;
            #[cfg(feature = "text")]
            {
                self.renderer.text.next_glyph = 0;
            }
        }
        result
    }

    /// Submits graphics and color encoding without copying pixels to CPU. A native
    /// window frame also acquires a FIFO image (which may block) and presents it.
    /// The next frame/readback waits on the submission fence before reusing memory.
    pub fn finish(self) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        #[cfg(feature = "text")]
        self.renderer.close_layers(self.clear)?;
        self.renderer.submit(self.clear, true)
    }

    /// Opens a layer: until the matching [`Self::pop_layer`], draws render into
    /// an image covering the layer's extent in the working format, composited
    /// on pop at its opacity. A positive backdrop blur first draws the blur of
    /// what is already drawn under the layer's shape over it; a blur whose
    /// scratch images exceed the memory budget is skipped and counted by
    /// [`Renderer::skipped_blurs`]. Each layer boundary is a submission the
    /// frame waits for. Needs `text`, whose image pipeline composites layers;
    /// without it this fails with [`Error::UnsupportedCommand`]. Layers still
    /// open when the frame finishes are composited first.
    pub fn push_layer(&mut self, layer: &aegle_scene::Layer) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        #[cfg(feature = "text")]
        let result = self.renderer.push_layer(layer, self.clear);
        #[cfg(not(feature = "text"))]
        let result = {
            let _ = layer;
            Err(Error::UnsupportedCommand)
        };
        self.failed = result.is_err();
        result
    }

    /// Composites the innermost open layer.
    pub fn pop_layer(&mut self) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        #[cfg(feature = "text")]
        let result = self.renderer.pop_layer(self.clear);
        #[cfg(not(feature = "text"))]
        let result = Err(Error::UnbalancedLayer);
        self.failed = result.is_err();
        result
    }
}

#[cfg(feature = "text")]
impl Renderer {
    /// Records one atlas-backed command within the remaining device budget.
    fn record(
        &mut self,
        scene: &Scene,
        command: aegle_scene::Command,
        state: aegle_gpu::State,
        view: [f32; 2],
    ) -> Result {
        let limits = crate::text::Limits {
            viewport: view,
            device: self.options.memory_budget - self.base_bytes(),
        };
        self.text.record(
            &self.device,
            &mut self.recording,
            scene,
            command,
            state,
            limits,
        )
    }
}
