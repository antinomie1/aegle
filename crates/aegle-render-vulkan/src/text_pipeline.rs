//! Optional atlas pipeline; its page descriptors are updated only after the fence.
#![allow(unsafe_code)]

use ash::vk;

use crate::{
    Error, Result,
    device::Device,
    geometry::Primitive,
    pipeline::{Pipeline, graphics},
};

pub(crate) struct TextPipeline {
    raw: ash::Device,
    pub layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    page_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
}

impl TextPipeline {
    pub fn new(device: &Device, geometry: &Pipeline, max_pages: u32) -> Result<Self> {
        if max_pages == 0 {
            return Err(Error::InvalidState("text atlas needs at least one page"));
        }
        let mut this = Self {
            raw: device.raw.clone(),
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            sampler: vk::Sampler::null(),
            page_layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            sets: Vec::new(),
        };
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        // SAFETY: Device and geometry outlive this owner; its Drop handles partial
        // creation. No optional sampler feature or descriptor indexing is used.
        unsafe {
            this.sampler = this.raw.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .min_lod(0.0)
                    .max_lod(0.0),
                None,
            )?;
            this.page_layout = this.raw.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            let layouts = [geometry.clip_layout(), this.page_layout];
            let ranges = [vk::PushConstantRange::default()
                .offset(0)
                .size(size_of::<Primitive>() as u32)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
            this.layout = this.raw.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&layouts)
                    .push_constant_ranges(&ranges),
                None,
            )?;
            let sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: max_pages,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: max_pages,
                },
            ];
            this.pool = this.raw.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(max_pages)
                    .pool_sizes(&sizes),
                None,
            )?;
            let mut page_layouts = Vec::new();
            page_layouts
                .try_reserve_exact(max_pages as usize)
                .map_err(|_| Error::Allocation)?;
            page_layouts.resize(max_pages as usize, this.page_layout);
            this.sets = this.raw.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(this.pool)
                    .set_layouts(&page_layouts),
            )?;
        }
        this.pipeline = graphics(
            &this.raw,
            geometry.passes[0],
            this.layout,
            true,
            include_bytes!(concat!(env!("OUT_DIR"), "/geometry.vert.spv")),
            include_bytes!(concat!(env!("OUT_DIR"), "/geometry.text.frag.spv")),
            c"fs_text",
        )?;
        Ok(this)
    }

    /// Page indices come from the atlas's configured fixed page range.
    pub fn set(&self, page: u32) -> vk::DescriptorSet {
        self.sets[page as usize]
    }

    /// Rebinds one page after previous GPU reads have completed. The atlas retains
    /// the image/view until the next fence and transitions it before shader reads.
    pub fn update(&self, page: u32, view: vk::ImageView) {
        let image = [vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler = [vk::DescriptorImageInfo::default().sampler(self.sampler)];
        let set = self.set(page);
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&image),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&sampler),
        ];
        // SAFETY: The caller fences prior uses; handles belong to this device,
        // writes match both layout bindings, and referenced objects outlive draws.
        unsafe { self.raw.update_descriptor_sets(&writes, &[]) };
    }
}

impl Drop for TextPipeline {
    fn drop(&mut self) {
        // SAFETY: Renderer fences all submitted work and drops this owner before
        // its shared geometry render pass and device. Null handles are permitted.
        unsafe {
            self.raw.destroy_pipeline(self.pipeline, None);
            self.raw.destroy_pipeline_layout(self.layout, None);
            self.raw.destroy_descriptor_pool(self.pool, None);
            self.raw
                .destroy_descriptor_set_layout(self.page_layout, None);
            self.raw.destroy_sampler(self.sampler, None);
        }
    }
}
