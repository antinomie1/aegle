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
        let extent = self.extent();
        let (renderer, clear) = (&mut *self.renderer, self.clear);
        let view = viewport(extent[0], extent[1], false);
        let result = (|| -> Result {
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
                                renderer.submit(clear, false)?;
                            }
                            result => break result?,
                        }
                    },
                    #[cfg(not(feature = "text"))]
                    Step::Command(..) => return Err(Error::UnsupportedCommand),
                }
                if renderer.recording.primitives.len() >= MAX_PRIMITIVES {
                    renderer.submit(clear, false)?;
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
        self.renderer.submit(self.clear, true)
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
