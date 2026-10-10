//! Offscreen layers and backdrop blur.
//!
//! A layer image has the working format of pass 0, so the geometry and text
//! pipelines draw into it through the compatible layer passes; it ends each
//! submission readable by shaders and is composited through a reserved
//! texture slot. A backdrop blur copies the target's sampled area into a
//! scratch image and runs the six shared box passes between two RGBA16F
//! images. Every boundary is a submission the frame waits for, like a frame
//! split by a full atlas, so images are reused without further hazards.
#![allow(unsafe_code)]

use aegle_gpu::{LayerPlan, composite, empty, plan_layer, shifted, viewport};
use aegle_scene::{Affine, Color, Layer, Rect};
use ash::vk;

use crate::{Error, Renderer, Result, memory::Image};

mod blur;
use blur::Scratch;

/// A layer image with its framebuffer for the layer passes.
pub(crate) struct LayerImage {
    framebuffer: vk::Framebuffer,
    image: Image,
    size: [u32; 2],
    raw: ash::Device,
}

impl Drop for LayerImage {
    fn drop(&mut self) {
        // SAFETY: the renderer waits for every submission using the image
        // before dropping it; the framebuffer goes before its image view.
        unsafe { self.raw.destroy_framebuffer(self.framebuffer, None) };
    }
}

struct Open {
    /// `None` when nothing of the layer can show; its draws are dropped.
    image: Option<LayerImage>,
    plan: LayerPlan,
    /// Top left in the frame's device coordinates.
    origin: [u32; 2],
    opacity: f32,
    /// A submission drew into the image this frame.
    loaded: bool,
}

/// Open layers and reusable images.
#[derive(Default)]
pub(crate) struct Layers {
    open: Vec<Open>,
    spare: Vec<LayerImage>,
    scratch: Option<Scratch>,
    /// Backdrop blurs skipped for exceeding the memory budget.
    pub skipped_blurs: u64,
}

impl Layers {
    /// Device bytes of every layer and scratch image.
    pub fn bytes(&self) -> u64 {
        let open = self.open.iter().filter_map(|o| o.image.as_ref());
        let layers: u64 = open.chain(&self.spare).map(|i| i.image.allocation).sum();
        let scratch = self.scratch.as_ref().map_or(0, Scratch::bytes);
        layers + scratch
    }
    /// Releases every image; the renderer has waited.
    pub fn clear(&mut self) {
        *self = Self {
            skipped_blurs: self.skipped_blurs,
            ..Self::default()
        };
    }
}

fn framebuffer(
    raw: &ash::Device,
    pass: vk::RenderPass,
    view: vk::ImageView,
    size: [u32; 2],
) -> Result<vk::Framebuffer> {
    let attachments = [view];
    // SAFETY: the view's image matches the pass format and this extent.
    Ok(unsafe {
        raw.create_framebuffer(
            &vk::FramebufferCreateInfo::default()
                .render_pass(pass)
                .attachments(&attachments)
                .width(size[0])
                .height(size[1])
                .layers(1),
            None,
        )?
    })
}

impl Renderer {
    /// Backdrop blurs skipped so far because their scratch images would
    /// exceed the memory budget.
    pub fn skipped_blurs(&self) -> u64 {
        self.layers.skipped_blurs
    }

    /// The frame-space origin and size of the current target, and whether a
    /// layer that cannot show is open.
    pub(crate) fn current(&self) -> ([u32; 2], [u32; 2], bool) {
        match self.layers.open.last() {
            Some(open) => match &open.image {
                Some(image) => (open.origin, image.size, false),
                None => ([0; 2], [1; 2], true),
            },
            None => {
                let target = self.target.as_ref().unwrap();
                ([0; 2], [target.width, target.height], false)
            }
        }
    }

    /// The transform shift, clip and viewport for a draw into the current target.
    pub(crate) fn placement(
        &self,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result<(Affine, Option<Rect>, [u32; 2], bool)> {
        let (origin, size, hidden) = self.current();
        let shift = Affine::translation(-(origin[0] as f32), -(origin[1] as f32));
        let clip = clip.map(|rect| shifted(rect, origin));
        Ok((transform.then(shift)?, clip, size, hidden))
    }

    /// Draws the records into the current target and continues the frame.
    pub(crate) fn flush_current(&mut self, clear: Color) -> Result {
        let Some(open) = self.layers.open.last_mut() else {
            return self.submit(clear, false);
        };
        let Some(image) = &open.image else {
            self.recording.primitives.clear();
            return Ok(());
        };
        let (framebuffer, size, resume) = (image.framebuffer, image.size, open.loaded);
        open.loaded = true;
        self.submit_layer(framebuffer, size, resume)
    }

    /// Draws the records into a layer image's framebuffer and waits for it.
    pub(crate) fn submit_layer(
        &mut self,
        framebuffer: vk::Framebuffer,
        extent: [u32; 2],
        resume: bool,
    ) -> Result {
        self.begin_submission()?;
        self.commands.render_layer(
            &self.pipeline,
            &self.recording,
            framebuffer,
            extent,
            resume,
            &self.text.pipeline,
        );
        self.commands.submit(self.device.queue)?;
        self.text.atlas.commit();
        self.busy = true;
        self.continue_frame()
    }

    pub(crate) fn push_layer(&mut self, layer: &Layer, clear: Color) -> Result {
        self.flush_current(clear)?;
        let (origin, size, hidden) = self.current();
        if hidden {
            let plan = plan_layer(layer, origin, [0; 2]);
            self.layers.open.push(Open {
                image: None,
                plan,
                origin,
                opacity: 0.0,
                loaded: false,
            });
            return Ok(());
        }
        if layer.backdrop_blur() > 0.0 {
            self.blur_backdrop(layer, origin, size, clear)?;
        }
        let plan = plan_layer(layer, origin, size);
        let extent = plan.extent;
        let image = if empty(extent) {
            None
        } else {
            let wanted = [extent[2] - extent[0], extent[3] - extent[1]];
            Some(
                match self.layers.spare.iter().position(|i| i.size == wanted) {
                    Some(index) => self.layers.spare.swap_remove(index),
                    None => self.layer_image(wanted)?,
                },
            )
        };
        self.layers.open.push(Open {
            image,
            plan,
            origin: [origin[0] + extent[0], origin[1] + extent[1]],
            opacity: layer.opacity(),
            loaded: false,
        });
        Ok(())
    }

    pub(crate) fn pop_layer(&mut self, clear: Color) -> Result {
        if self.layers.open.is_empty() {
            return Err(Error::UnbalancedLayer);
        }
        self.flush_current(clear)?;
        let open = self.layers.open.pop().unwrap();
        let Some(image) = open.image else {
            return Ok(());
        };
        if open.loaded {
            let at = [open.plan.extent[0], open.plan.extent[1]];
            let page = self.bind_internal(image.image.view, clear)?;
            let (_, size, _) = self.current();
            let view = viewport(size[0], size[1], false);
            composite(
                &mut self.recording,
                image.size,
                at,
                open.plan.clip,
                None,
                open.opacity,
                view,
                page,
            )?;
        }
        if self.layers.spare.len() < 8 {
            self.layers.spare.push(image);
        }
        Ok(())
    }

    /// Pops every open layer, before the frame's last submission.
    pub(crate) fn close_layers(&mut self, clear: Color) -> Result {
        while !self.layers.open.is_empty() {
            self.pop_layer(clear)?;
        }
        Ok(())
    }

    /// Starts a frame with no layers open.
    pub(crate) fn begin_layers(&mut self) {
        self.layers.open.clear();
    }

    /// Binds an internal image to a free texture slot, first drawing what the
    /// target holds when every slot is taken.
    fn bind_internal(&mut self, view: vk::ImageView, clear: Color) -> Result<u32> {
        if let Some(page) = self.text.bind_internal(view) {
            return Ok(page);
        }
        self.flush_current(clear)?;
        self.text.bind_internal(view).ok_or(Error::TooManyTextures)
    }

    fn layer_image(&self, size: [u32; 2]) -> Result<LayerImage> {
        let image = Image::new(
            &self.device,
            size[0],
            size[1],
            self.pipeline.working,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC,
            self.remaining(),
        )?;
        let raw = self.device.raw.clone();
        let framebuffer = framebuffer(&raw, self.pipeline.layer_passes[0], image.view, size)?;
        Ok(LayerImage {
            framebuffer,
            image,
            size,
            raw,
        })
    }
}
