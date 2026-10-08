//! Backdrop blur: the sampled area is copied into scratch and blurred by
//! the six shared box passes between two working-format images.
#![allow(unsafe_code)]

use aegle_gpu::{composite, plan_blur, viewport};
use aegle_scene::{Affine, Color, Layer};
use ash::vk;

use super::framebuffer;
use crate::{Error, Renderer, Result, memory::Image};

/// Images of one size for a blur: the copied backdrop and the two images
/// the passes alternate between, with a sampling set for each.
pub(super) struct Scratch {
    copy: Image,
    passes: [Image; 2],
    framebuffers: [vk::Framebuffer; 2],
    pool: vk::DescriptorPool,
    sets: [vk::DescriptorSet; 3],
    size: [u32; 2],
    raw: ash::Device,
}

impl Scratch {
    pub(super) fn bytes(&self) -> u64 {
        self.copy.allocation + self.passes.iter().map(|i| i.allocation).sum::<u64>()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // SAFETY: waited for like layer images; the pool frees its sets.
        unsafe {
            for framebuffer in self.framebuffers {
                self.raw.destroy_framebuffer(framebuffer, None);
            }
            self.raw.destroy_descriptor_pool(self.pool, None);
        }
    }
}

impl Renderer {
    fn scratch(&self, size: [u32; 2]) -> Result<Scratch> {
        let budget = self.remaining();
        let copy = Image::new(
            &self.device,
            size[0],
            size[1],
            self.pipeline.working,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            budget,
        )?;
        let pass = |budget| {
            Image::new(
                &self.device,
                size[0],
                size[1],
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                budget,
            )
        };
        let first = pass(budget - copy.allocation)?;
        let second = pass(budget - copy.allocation - first.allocation)?;
        let raw = self.device.raw.clone();
        let mut scratch = Scratch {
            copy,
            passes: [first, second],
            framebuffers: [vk::Framebuffer::null(); 2],
            pool: vk::DescriptorPool::null(),
            sets: [vk::DescriptorSet::null(); 3],
            size,
            raw: raw.clone(),
        };
        for index in 0..2 {
            let view = scratch.passes[index].view;
            scratch.framebuffers[index] = framebuffer(&raw, self.pipeline.blur_pass, view, size)?;
        }
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: 3,
        }];
        let layouts = [self.pipeline.image_layout(); 3];
        // SAFETY: the layout belongs to this device; the pool is owned by
        // `scratch`, whose Drop destroys it after every use was waited for.
        unsafe {
            scratch.pool = raw.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(3)
                    .pool_sizes(&sizes),
                None,
            )?;
            let sets = raw.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(scratch.pool)
                    .set_layouts(&layouts),
            )?;
            scratch.sets.copy_from_slice(&sets);
            let views = [
                scratch.copy.view,
                scratch.passes[0].view,
                scratch.passes[1].view,
            ];
            for (set, view) in scratch.sets.into_iter().zip(views) {
                let info = [vk::DescriptorImageInfo::default()
                    .image_view(view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let write = vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&info);
                raw.update_descriptor_sets(&[write], &[]);
            }
        }
        Ok(scratch)
    }

    /// The current target's image and the layout its last submission left.
    fn source(&self) -> Result<(vk::Image, vk::ImageLayout)> {
        if let Some(open) = self.layers.open.last() {
            let image = open.image.as_ref().unwrap();
            return Ok((
                image.image.handle,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ));
        }
        let target = self.target.as_ref().unwrap();
        if let Some(linear) = &target.linear {
            return Ok((linear.handle, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL));
        }
        #[cfg(feature = "window")]
        if let (Some(chain), Some(index)) = (&self.swapchain, self.acquired_index()) {
            if !chain.readable {
                return Err(Error::Unsupported(
                    "backdrop blur needs a swapchain readable by transfers",
                ));
            }
            return Ok((
                chain.images[index as usize],
                vk::ImageLayout::PRESENT_SRC_KHR,
            ));
        }
        Err(Error::InvalidState("no drawn target to blur"))
    }

    /// Draws the blur of the current target under the layer shape over it.
    pub(super) fn blur_backdrop(
        &mut self,
        layer: &Layer,
        origin: [u32; 2],
        size: [u32; 2],
        clear: Color,
    ) -> Result {
        let Some(plan) = plan_blur(layer, origin, size) else {
            return Ok(());
        };
        let sampled = plan.sampled;
        let extent = [sampled[2] - sampled[0], sampled[3] - sampled[1]];
        if self
            .layers
            .scratch
            .as_ref()
            .is_none_or(|s| s.size != extent)
        {
            self.layers.scratch = None;
            match self.scratch(extent) {
                Ok(scratch) => self.layers.scratch = Some(scratch),
                Err(Error::Budget { .. }) => {
                    self.layers.skipped_blurs += 1;
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
        let (source, layout) = self.source()?;
        self.commands.begin()?;
        self.record_blur(source, layout, sampled, &plan.passes);
        self.commands.submit(self.device.queue)?;
        self.busy = true;
        self.continue_frame()?;
        let view = self.layers.scratch.as_ref().unwrap().passes[1].view;
        let page = self.bind_internal(view, clear)?;
        let shift = Affine::translation(-(origin[0] as f32), -(origin[1] as f32))?;
        let shape = (layer.shape(), layer.transform().then(shift)?);
        let at = [sampled[0], sampled[1]];
        let view = viewport(size[0], size[1], false);
        let opacity = layer.opacity();
        composite(
            &mut self.recording,
            extent,
            at,
            plan.area,
            Some(shape),
            opacity,
            view,
            page,
        )?;
        // The scratch images are reused by the next blur: draw this one now.
        self.flush_current(clear)
    }

    /// Copies the sampled area into scratch and records the six box passes.
    fn record_blur(
        &self,
        source: vk::Image,
        layout: vk::ImageLayout,
        sampled: [u32; 4],
        passes: &[u32; 6],
    ) {
        let scratch = self.layers.scratch.as_ref().unwrap();
        let raw = &self.device.raw;
        let buffer = self.commands.buffer;
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let barrier = |image, from, to, src, dst| {
            vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(from)
                .new_layout(to)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .subresource_range(range)
        };
        let layers = vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        };
        let size = scratch.size;
        let copy = [vk::ImageCopy::default()
            .src_subresource(layers)
            .src_offset(vk::Offset3D {
                x: sampled[0] as i32,
                y: sampled[1] as i32,
                z: 0,
            })
            .dst_subresource(layers)
            .extent(vk::Extent3D {
                width: size[0],
                height: size[1],
                depth: 1,
            })];
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: size[0],
                height: size[1],
            },
        };
        let viewport = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: size[0] as f32,
            height: size[1] as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let values = [vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.0; 4] },
        }];
        let read = vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE;
        // SAFETY: the frame waited for every earlier submission, so the source
        // holds its finished pixels in `layout` and the scratch images are
        // idle. Barriers move the source to TRANSFER_SRC and back, and the
        // copy into SHADER_READ_ONLY; each blur pass ends shader-readable
        // with its render pass dependency ordering the next pass's reads.
        unsafe {
            let before = [
                barrier(
                    source,
                    layout,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    read,
                    vk::AccessFlags::TRANSFER_READ,
                ),
                barrier(
                    scratch.copy.handle,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_WRITE,
                ),
            ];
            raw.cmd_pipeline_barrier(
                buffer,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &before,
            );
            raw.cmd_copy_image(
                buffer,
                source,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                scratch.copy.handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &copy,
            );
            let after = [
                barrier(
                    source,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    layout,
                    vk::AccessFlags::TRANSFER_READ,
                    read,
                ),
                barrier(
                    scratch.copy.handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                ),
            ];
            raw.cmd_pipeline_barrier(
                buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &after,
            );
            raw.cmd_set_viewport(buffer, 0, &viewport);
            raw.cmd_set_scissor(buffer, 0, &[area]);
            for (index, &pass) in passes.iter().enumerate() {
                // The copy feeds the first pass; the others alternate.
                let from = if index == 0 { 0 } else { 2 - index % 2 };
                let to = index % 2;
                raw.cmd_begin_render_pass(
                    buffer,
                    &vk::RenderPassBeginInfo::default()
                        .render_pass(self.pipeline.blur_pass)
                        .framebuffer(scratch.framebuffers[to])
                        .render_area(area)
                        .clear_values(&values),
                    vk::SubpassContents::INLINE,
                );
                raw.cmd_bind_pipeline(buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline.blur);
                raw.cmd_bind_descriptor_sets(
                    buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline.layouts[1],
                    0,
                    &[scratch.sets[from]],
                    &[],
                );
                raw.cmd_draw(buffer, 3, 1, 0, pass);
                raw.cmd_end_render_pass(buffer);
            }
        }
    }
}
