//! FIFO swapchain with per-image presentation semaphores and bounded image estimates.
#![allow(unsafe_code)]
use crate::{Error, Result, device::Device, pipeline::Pipeline};
use ash::vk;

pub(crate) struct Swapchain {
    raw: ash::Device,
    loader: ash::khr::swapchain::Device,
    handle: vk::SwapchainKHR,
    views: Vec<vk::ImageView>,
    pub frames: Vec<vk::Framebuffer>,
    signals: Vec<vk::Semaphore>,
    acquire_fence: vk::Fence,
    pub extent: vk::Extent2D,
    pub bytes: u64,
    pub dirty: bool,
    pub transparent: bool,
}

impl Swapchain {
    pub fn new(
        device: &Device,
        surface: &crate::surface::Surface,
        pipeline: &Pipeline,
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<Option<Self>> {
        // SAFETY: Native owners and instance keep this surface valid throughout.
        let caps = unsafe {
            surface
                .loader
                .get_physical_device_surface_capabilities(device.physical, surface.handle)?
        };
        if !caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        {
            return Err(Error::Unsupported(
                "surface color attachment usage is required",
            ));
        }
        let extent = if caps.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        } else {
            caps.current_extent
        };
        if extent.width == 0 || extent.height == 0 {
            return Ok(None);
        }
        let limits = device.properties.limits;
        if extent.width > limits.max_image_dimension2_d
            || extent.height > limits.max_image_dimension2_d
            || extent.width > limits.max_framebuffer_width
            || extent.height > limits.max_framebuffer_height
            || extent.width > limits.max_viewport_dimensions[0]
            || extent.height > limits.max_viewport_dimensions[1]
            || extent.width as f32 > limits.viewport_bounds_range[1]
            || extent.height as f32 > limits.viewport_bounds_range[1]
        {
            return Err(Error::InvalidSize);
        }
        let count = caps.min_image_count.max(2);
        if caps.max_image_count != 0 && count > caps.max_image_count {
            return Err(Error::Unsupported("surface requires fewer than two images"));
        }
        let estimate = |count: u64| {
            u64::from(extent.width)
                .checked_mul(u64::from(extent.height))
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|bytes| bytes.checked_mul(count))
                .ok_or(Error::InvalidSize)
        };
        // Direct sRGB output stores encoded premultiplied-linear values, which are
        // exact only for opaque pixels; it prefers opaque composition.
        let mut alphas = [
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::OPAQUE,
        ];
        if pipeline.direct {
            alphas.reverse();
        }
        let alpha = alphas
            .into_iter()
            .find(|alpha| caps.supported_composite_alpha.contains(*alpha))
            .ok_or(Error::Unsupported(
                "surface requires premultiplied or opaque composition",
            ))?;
        let format = surface.format(device.physical, transparent)?;
        if format.format != pipeline.output_format {
            return Err(Error::Unsupported(
                "surface color format changed; recreate window renderer",
            ));
        }
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.handle)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let mut this = Self {
            raw: device.raw.clone(),
            loader: ash::khr::swapchain::Device::new(&device.instance, &device.raw),
            handle: vk::SwapchainKHR::null(),
            views: Vec::new(),
            frames: Vec::new(),
            signals: Vec::new(),
            acquire_fence: vk::Fence::null(),
            extent,
            bytes: 0,
            dirty: false,
            transparent: !pipeline.direct && alpha == vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        };
        // SAFETY: The selected graphics family can present; formats/caps were queried.
        // Partial initialization is reclaimed by Drop, before the device/surface.
        unsafe {
            this.handle = this.loader.create_swapchain(&info, None)?;
            let images = this.loader.get_swapchain_images(this.handle)?;
            this.bytes = estimate(images.len() as u64)?;
            this.acquire_fence = this
                .raw
                .create_fence(&vk::FenceCreateInfo::default(), None)?;
            for image in images {
                let view = this.raw.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )?;
                this.views.push(view);
                this.frames.push(
                    this.raw.create_framebuffer(
                        &vk::FramebufferCreateInfo::default()
                            .render_pass(pipeline.passes[usize::from(!pipeline.direct)])
                            .attachments(&[view])
                            .width(extent.width)
                            .height(extent.height)
                            .layers(1),
                        None,
                    )?,
                );
                this.signals.push(
                    this.raw
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
                );
            }
        }
        Ok(Some(this))
    }
    pub fn acquire(&mut self) -> Result<(u32, vk::Semaphore, bool)> {
        self.dirty = true; // An error after acquisition requires teardown, never unsafe reuse.
        // SAFETY: Previous acquisition was waited on before rendering; one image is
        // acquired at a time. Waiting the acquire fence makes image access available
        // before queue submission without needing another binary semaphore.
        let result = unsafe {
            self.raw.reset_fences(&[self.acquire_fence])?;
            self.loader.acquire_next_image(
                self.handle,
                u64::MAX,
                vk::Semaphore::null(),
                self.acquire_fence,
            )
        };
        let (index, suboptimal) = match result {
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Err(Error::SurfaceOutOfDate),
            value => value?,
        };
        // SAFETY: Successful acquisition submitted a signal operation on this fence.
        unsafe {
            self.raw
                .wait_for_fences(&[self.acquire_fence], true, u64::MAX)?;
        }
        Ok((index, self.signals[index as usize], suboptimal))
    }
    pub fn present(&mut self, queue: vk::Queue, index: u32, suboptimal: bool) -> Result {
        let waits = [self.signals[index as usize]];
        let chains = [self.handle];
        let indices = [index];
        // SAFETY: The matching image was acquired and a successful graphics submit
        // will signal its semaphore. Reacquiring this image gates semaphore reuse.
        let result = unsafe {
            self.loader.queue_present(
                queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&waits)
                    .swapchains(&chains)
                    .image_indices(&indices),
            )
        };
        match result {
            Ok(changed) => {
                self.dirty = changed || suboptimal;
                Ok(())
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Err(Error::SurfaceOutOfDate),
            Err(error) => Err(error.into()),
        }
    }
}
impl Drop for Swapchain {
    fn drop(&mut self) {
        // SAFETY: Renderer waits for device idle before replacing/dropping a live
        // swapchain. Partial initialization has no submitted commands or presents.
        unsafe {
            for frame in &self.frames {
                self.raw.destroy_framebuffer(*frame, None);
            }
            for view in &self.views {
                self.raw.destroy_image_view(*view, None);
            }
            for signal in &self.signals {
                self.raw.destroy_semaphore(*signal, None);
            }
            self.raw.destroy_fence(self.acquire_fence, None);
            self.loader.destroy_swapchain(self.handle, None);
        }
    }
}
