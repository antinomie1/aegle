//! The linear working attachment and final premultiplied sRGB image. A direct
//! window target owns neither: pass 0 renders into the acquired swapchain image.
#![allow(unsafe_code)]

use crate::{Result, device::Device, memory::Image, pipeline::Pipeline};
use ash::vk;

pub(crate) struct Target {
    raw: ash::Device,
    pub frames: [vk::Framebuffer; 2],
    pub linear: Option<Image>,
    pub output: Option<Image>,
    pub width: u32,
    pub height: u32,
}

impl Target {
    pub fn new(
        device: &Device,
        pipeline: &Pipeline,
        width: u32,
        height: u32,
        budget: u64,
    ) -> Result<Self> {
        let linear = if pipeline.direct {
            None
        } else {
            Some(Image::new(
                device,
                width,
                height,
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                budget,
            )?)
        };
        let window = false;
        #[cfg(feature = "window")]
        let window = window || device.surface.is_some();
        let output = if window {
            None
        } else {
            Some(Image::new(
                device,
                width,
                height,
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                budget - linear.as_ref().map_or(0, |image| image.allocation),
            )?)
        };
        let mut this = Self {
            raw: device.raw.clone(),
            frames: [vk::Framebuffer::null(); 2],
            linear,
            output,
            width,
            height,
        };
        for (i, view) in [&this.linear, &this.output]
            .into_iter()
            .enumerate()
            .filter_map(|(i, image)| image.as_ref().map(|image| (i, image.view)))
        {
            let attachments = [view];
            // SAFETY: Images match the render pass format/sample count and extent;
            // their views remain owned alongside these framebuffers.
            this.frames[i] = unsafe {
                this.raw.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(pipeline.passes[i])
                        .attachments(&attachments)
                        .width(width)
                        .height(height)
                        .layers(1),
                    None,
                )?
            };
        }
        Ok(this)
    }
    pub fn bytes(&self) -> u64 {
        [&self.linear, &self.output]
            .into_iter()
            .flatten()
            .map(|image| image.allocation)
            .sum()
    }
    pub fn area(&self) -> vk::Rect2D {
        vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: self.width,
                height: self.height,
            },
        }
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        // SAFETY: Renderer waits for its fence before dropping or replacing this
        // target. Framebuffers are destroyed before the Image fields and device.
        unsafe {
            for frame in self.frames {
                self.raw.destroy_framebuffer(frame, None);
            }
        }
    }
}
