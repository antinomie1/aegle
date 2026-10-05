//! Bounded glyph patches and their single transient Vulkan transfer buffer.
#![allow(unsafe_code)]

use aegle_glyph::{Content, Glyph};
use aegle_types::color_math::SrgbTransfer;
use ash::vk;

use crate::{Error, Result, device::Device, memory::Buffer};

pub(crate) struct Region {
    page: u32,
    copy: vk::BufferImageCopy,
}

#[derive(Default)]
pub(crate) struct Uploads {
    pub bytes: Vec<u8>,
    pub regions: Vec<Region>,
    staging: Option<Buffer>,
}
impl Uploads {
    pub fn begin_frame(&mut self) {
        self.retire();
        self.commit();
    }

    pub fn retire(&mut self) {
        // Renderer has fenced the prior upload before releasing its transfer buffer.
        self.staging = None;
    }

    pub fn commit(&mut self) {
        self.bytes.clear();
        self.regions.clear();
    }

    pub fn append(
        &mut self,
        page: u32,
        xy: [u32; 2],
        glyph: Glyph<'_>,
        limit: usize,
        max_regions: u32,
    ) -> Result {
        let offset = self
            .bytes
            .len()
            .checked_add(3)
            .map(|n| n & !3)
            .ok_or(Error::Allocation)?;
        let required = offset
            .checked_add(glyph.data.len())
            .ok_or(Error::Allocation)?;
        if required > limit {
            return Err(Error::Budget {
                required: required as u64,
                limit: limit as u64,
            });
        }
        if self.regions.len() >= max_regions as usize {
            return Err(Error::AtlasFull);
        }
        reserve(&mut self.bytes, required, limit)?;
        let regions = self.regions.len() + 1;
        reserve(&mut self.regions, regions, max_regions as usize)?;
        self.bytes.resize(required, 0);
        match glyph.content {
            Content::Mask => self.bytes[offset..].copy_from_slice(glyph.data),
            Content::Color => {
                // The sRGB texture must decode to premultiplied linear values
                // before filtering. Alpha stays linear, including transparent edges.
                let transfer = SrgbTransfer::get();
                for (source, dest) in glyph
                    .data
                    .chunks_exact(4)
                    .zip(self.bytes[offset..].chunks_exact_mut(4))
                {
                    let alpha = source[3] as f32 / 255.0;
                    for c in 0..3 {
                        dest[c] = (transfer
                            .encode(transfer.decode(source[c] as f32 / 255.0) * alpha)
                            * 255.0)
                            .round() as u8;
                    }
                    dest[3] = source[3];
                }
            }
        }
        self.regions.push(Region {
            page,
            copy: vk::BufferImageCopy::default()
                .buffer_offset(offset as u64)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D {
                    x: xy[0] as i32,
                    y: xy[1] as i32,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width: glyph.placement.width,
                    height: glyph.placement.height,
                    depth: 1,
                }),
        });
        Ok(())
    }

    pub fn prepare(&mut self, device: &Device, budget: u64) -> Result {
        if self.bytes.is_empty() {
            return Ok(());
        }
        self.staging = None;
        let mut buffer = Buffer::new(
            device,
            self.bytes.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
            budget,
        )?;
        buffer.write(0, &self.bytes)?;
        self.staging = Some(buffer);
        Ok(())
    }

    pub fn device_bytes(&self) -> u64 {
        self.staging.as_ref().map_or(0, |buffer| buffer.allocation)
    }

    pub fn record(
        &self,
        raw: &ash::Device,
        command: vk::CommandBuffer,
        page: u32,
        image: vk::Image,
        initialized: bool,
        reset: bool,
    ) {
        let buffer = self
            .staging
            .as_ref()
            .expect("prepare_upload precedes recording");
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let barrier = |old, new, source, destination| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(source)
                .dst_access_mask(destination)
                .old_layout(old)
                .new_layout(new)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
        };
        // SAFETY: Renderer has fenced previous use, and this live command buffer
        // is recording outside a render pass. The page has one color layer/mip,
        // SAMPLED|TRANSFER_DST usage and the tracked committed layout. Upload
        // offsets are four-byte aligned; tightly packed patches fit their page
        // and staging range. Shelf packing makes every patch disjoint with a
        // one-pixel gutter. All referenced objects survive the submission fence.
        unsafe {
            raw.cmd_pipeline_barrier(
                command,
                if initialized {
                    vk::PipelineStageFlags::FRAGMENT_SHADER
                } else {
                    vk::PipelineStageFlags::TOP_OF_PIPE
                },
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier(
                    if initialized {
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                    } else {
                        vk::ImageLayout::UNDEFINED
                    },
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    if initialized {
                        vk::AccessFlags::SHADER_READ
                    } else {
                        vk::AccessFlags::empty()
                    },
                    vk::AccessFlags::TRANSFER_WRITE,
                )],
            );
            if reset {
                raw.cmd_clear_color_image(
                    command,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue { float32: [0.0; 4] },
                    &[range],
                );
                // Order overlapping clear and patch writes. Unused shelf space
                // remains transparent without a full-page CPU zero buffer.
                raw.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier(
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::TRANSFER_WRITE,
                    )],
                );
            }
            for region in self.regions.iter().filter(|region| region.page == page) {
                raw.cmd_copy_buffer_to_image(
                    command,
                    buffer.handle,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region.copy],
                );
            }
            raw.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier(
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                )],
            );
        }
    }
}

// Grow geometrically within an already-validated count/byte ceiling, avoiding a
// reallocation for every new glyph while retaining a strict capacity bound.
fn reserve<T>(values: &mut Vec<T>, required: usize, limit: usize) -> Result {
    if required > values.capacity() {
        let capacity = required.max(values.capacity().saturating_mul(2)).min(limit);
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| Error::Allocation)?;
    }
    Ok(())
}
