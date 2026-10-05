//! One reusable command buffer and fence, with no background work or polling.
#![allow(unsafe_code)]

use crate::{Result, device::Device, geometry::Recording, pipeline::Pipeline, target::Target};
use ash::vk;

pub(crate) struct Commands {
    raw: ash::Device,
    pub buffer: vk::CommandBuffer,
    pub fence: vk::Fence,
    pool: vk::CommandPool,
}

impl Commands {
    pub fn new(device: &Device) -> Result<Self> {
        let mut this = Self {
            raw: device.raw.clone(),
            buffer: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            pool: vk::CommandPool::null(),
        };
        // SAFETY: The family was selected from this device's graphics queues.
        // Partial construction is cleaned by this owner's Drop implementation.
        unsafe {
            this.pool = this.raw.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(device.family),
                None,
            )?;
            this.fence = this
                .raw
                .create_fence(&vk::FenceCreateInfo::default(), None)?;
            this.buffer = this.raw.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(this.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0];
        }
        Ok(this)
    }

    pub fn begin(&self) -> Result {
        // SAFETY: Renderer has waited for the previous submission before reuse.
        unsafe {
            self.raw.reset_fences(&[self.fence])?;
            self.raw
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())?;
            self.raw.begin_command_buffer(
                self.buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }
        Ok(())
    }

    pub fn render(
        &self,
        target: &Target,
        pipeline: &Pipeline,
        recording: &Recording,
        clear: [f32; 4],
        output: vk::Framebuffer,
        #[cfg(feature = "text")] text: &crate::text_pipeline::TextPipeline,
    ) {
        let viewport = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: target.width as f32,
            height: target.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        // SAFETY: Command buffer is recording on the graphics queue. All objects
        // belong to the same live device and remain valid until its fence signals.
        // Instance ranges index the uploaded primitive buffer, whose bounds are
        // already clipped to the target. Descriptor ranges were updated.
        unsafe {
            self.raw.cmd_set_viewport(self.buffer, 0, &viewport);
            let passes = if pipeline.direct { 1 } else { 2 };
            for pass in 0..passes {
                let values = [vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: if pass == 0 { clear } else { [0.0; 4] },
                    },
                }];
                self.raw.cmd_begin_render_pass(
                    self.buffer,
                    &vk::RenderPassBeginInfo::default()
                        .render_pass(pipeline.passes[pass])
                        .framebuffer(if pass == 0 && !pipeline.direct {
                            target.frames[0]
                        } else {
                            output
                        })
                        .render_area(target.area())
                        .clear_values(&values),
                    vk::SubpassContents::INLINE,
                );
                self.raw.cmd_bind_pipeline(
                    self.buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline.pipelines[pass],
                );
                self.raw.cmd_bind_descriptor_sets(
                    self.buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline.layouts[pass],
                    0,
                    &[pipeline.sets[pass]],
                    &[],
                );
                self.raw.cmd_set_scissor(self.buffer, 0, &[target.area()]);
                if pass == 1 {
                    self.raw.cmd_draw(self.buffer, 3, 1, 0, 0);
                    self.raw.cmd_end_render_pass(self.buffer);
                    continue;
                }
                // Adjacent records sharing a pipeline and atlas page form one draw.
                let key = |primitive: &crate::geometry::Primitive| {
                    (primitive.header[1] != 0, primitive.header[2])
                };
                let primitives = &recording.primitives;
                let mut first = 0;
                #[cfg(feature = "text")]
                let mut current = (false, u32::MAX);
                while first < primitives.len() {
                    let batch = key(&primitives[first]);
                    let count = primitives[first..]
                        .iter()
                        .take_while(|primitive| key(primitive) == batch)
                        .count();
                    #[cfg(feature = "text")]
                    {
                        if current.0 != batch.0 {
                            self.raw.cmd_bind_pipeline(
                                self.buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                if batch.0 {
                                    text.pipeline
                                } else {
                                    pipeline.pipelines[0]
                                },
                            );
                            current.0 = batch.0;
                        }
                        if batch.0 && current.1 != batch.1 {
                            self.raw.cmd_bind_descriptor_sets(
                                self.buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                text.layout,
                                1,
                                &[text.set(batch.1)],
                                &[],
                            );
                            current.1 = batch.1;
                        }
                    }
                    self.raw
                        .cmd_draw(self.buffer, 6, count as u32, 0, first as u32);
                    first += count;
                }
                self.raw.cmd_end_render_pass(self.buffer);
            }
        }
    }

    pub fn copy(&self, target: &Target, buffer: vk::Buffer, bytes: u64) {
        let copy = [vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D {
                width: target.width,
                height: target.height,
                depth: 1,
            })];
        let barriers = [vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(buffer)
            .offset(0)
            .size(bytes)];
        // SAFETY: Target's completed render pass left output in TRANSFER_SRC_OPTIMAL.
        // The buffer is TRANSFER_DST and has at least the tightly packed image bytes.
        // The subsequent fence plus HOST_READ barrier precedes mapped CPU access.
        unsafe {
            self.raw.cmd_copy_image_to_buffer(
                self.buffer,
                target.output.as_ref().unwrap().handle,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer,
                &copy,
            );
            self.raw.cmd_pipeline_barrier(
                self.buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &barriers,
                &[],
            );
        }
    }

    pub fn submit(&self, queue: vk::Queue) -> Result {
        self.submit_signal(queue, &[])
    }

    pub fn submit_signal(&self, queue: vk::Queue, signals: &[vk::Semaphore]) -> Result {
        let buffers = [self.buffer];
        let submits = [vk::SubmitInfo::default()
            .command_buffers(&buffers)
            .signal_semaphores(signals)];
        // SAFETY: Exclusive Renderer access serializes queue calls. The buffer is
        // recording and its reset fence is not associated with another submission.
        unsafe {
            self.raw.end_command_buffer(self.buffer)?;
            self.raw.queue_submit(queue, &submits, self.fence)?;
        }
        Ok(())
    }

    pub fn wait(&self) -> Result {
        // SAFETY: Called only after a successful submission associated this fence.
        unsafe {
            self.raw.wait_for_fences(&[self.fence], true, u64::MAX)?;
        }
        Ok(())
    }
}
impl Drop for Commands {
    fn drop(&mut self) {
        // SAFETY: Renderer waits before destruction; pool owns its command buffer.
        unsafe {
            self.raw.destroy_fence(self.fence, None);
            self.raw.destroy_command_pool(self.pool, None);
        }
    }
}
