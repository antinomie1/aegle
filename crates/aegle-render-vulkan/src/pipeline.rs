//! Vulkan pipeline objects; all handles share the renderer's device lifetime.
#![allow(unsafe_code)]

use crate::{Result, device::Device};
use ash::vk;

pub(crate) struct Pipeline {
    raw: ash::Device,
    #[cfg(feature = "window")]
    pub output_format: vk::Format,
    /// Pass 0 blends directly into an sRGB swapchain image; no encoding pass.
    pub direct: bool,
    /// Draws into a swapchain instead of an offscreen output image.
    pub window: bool,
    pub passes: [vk::RenderPass; 2],
    pub layouts: [vk::PipelineLayout; 2],
    pub pipelines: [vk::Pipeline; 2],
    pub sets: [vk::DescriptorSet; 2],
    set_layouts: [vk::DescriptorSetLayout; 2],
    pool: vk::DescriptorPool,
}

impl Pipeline {
    pub fn new(
        device: &Device,
        #[cfg(feature = "window")] surface: Option<&crate::surface::Surface>,
        #[cfg(feature = "window")] transparent: bool,
    ) -> Result<Self> {
        let format = vk::Format::R8G8B8A8_UNORM;
        let window = false;
        #[cfg(feature = "window")]
        let (format, window) = if let Some(surface) = surface {
            (surface.format(device.physical, transparent)?.format, true)
        } else {
            (format, window)
        };
        // sRGB attachments blend in linear light, matching the RGBA16F path for
        // opaque output while avoiding its image, bandwidth and second pass.
        let direct = matches!(
            format,
            vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB
        );
        let mut this = Self {
            raw: device.raw.clone(),
            #[cfg(feature = "window")]
            output_format: format,
            direct,
            window,
            passes: [vk::RenderPass::null(); 2],
            layouts: [vk::PipelineLayout::null(); 2],
            pipelines: [vk::Pipeline::null(); 2],
            sets: [vk::DescriptorSet::null(); 2],
            set_layouts: [vk::DescriptorSetLayout::null(); 2],
            pool: vk::DescriptorPool::null(),
        };
        let binding = |binding, ty, stages| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(ty)
                .descriptor_count(1)
                .stage_flags(stages)
        };
        let storage = vk::DescriptorType::STORAGE_BUFFER;
        let fragment = vk::ShaderStageFlags::FRAGMENT;
        // Set 0 holds clips and instanced primitive records; set 1 the resolve input.
        let bindings: [&[_]; 2] = [
            &[
                binding(0, storage, fragment),
                binding(1, storage, fragment | vk::ShaderStageFlags::VERTEX),
            ],
            &[binding(0, vk::DescriptorType::SAMPLED_IMAGE, fragment)],
        ];
        // SAFETY: Device remains live; slices only back synchronous creation calls.
        // The partially constructed owner destroys every successfully created handle.
        unsafe {
            for (i, bindings) in bindings.into_iter().enumerate() {
                this.set_layouts[i] = this.raw.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
                    None,
                )?;
                let layouts = [this.set_layouts[i]];
                this.layouts[i] = this.raw.create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts),
                    None,
                )?;
            }
            let sizes = [
                vk::DescriptorPoolSize {
                    ty: storage,
                    descriptor_count: 2,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: 1,
                },
            ];
            this.pool = this.raw.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(2)
                    .pool_sizes(&sizes),
                None,
            )?;
            let sets = this.raw.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(this.pool)
                    .set_layouts(&this.set_layouts),
            )?;
            this.sets.copy_from_slice(&sets);
        }
        this.passes[0] = if direct {
            render_pass(
                &this.raw,
                format,
                vk::ImageLayout::PRESENT_SRC_KHR,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::AccessFlags::empty(),
            )?
        } else {
            render_pass(
                &this.raw,
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_READ,
            )?
        };
        this.pipelines[0] = graphics(
            &this.raw,
            this.passes[0],
            this.layouts[0],
            true,
            include_bytes!(concat!(env!("OUT_DIR"), "/geometry.vert.spv")),
            include_bytes!(concat!(env!("OUT_DIR"), "/geometry.frag.spv")),
            c"fs_main",
        )?;
        if direct {
            return Ok(this);
        }
        this.passes[1] = render_pass(
            &this.raw,
            format,
            if window {
                vk::ImageLayout::PRESENT_SRC_KHR
            } else {
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            },
            if window {
                vk::PipelineStageFlags::BOTTOM_OF_PIPE
            } else {
                vk::PipelineStageFlags::TRANSFER
            },
            if window {
                vk::AccessFlags::empty()
            } else {
                vk::AccessFlags::TRANSFER_READ
            },
        )?;
        this.pipelines[1] = graphics(
            &this.raw,
            this.passes[1],
            this.layouts[1],
            false,
            include_bytes!(concat!(env!("OUT_DIR"), "/resolve.vert.spv")),
            include_bytes!(concat!(env!("OUT_DIR"), "/resolve.frag.spv")),
            c"fs_main",
        )?;
        Ok(this)
    }

    #[cfg(feature = "text")]
    pub fn clip_layout(&self) -> vk::DescriptorSetLayout {
        self.set_layouts[0]
    }

    /// Binds whole clip and primitive buffers plus any resolve input.
    pub fn update(&self, clips: vk::Buffer, primitives: vk::Buffer, image: Option<vk::ImageView>) {
        let info = |buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)]
        };
        let (clips, primitives) = (info(clips), info(primitives));
        let texture = [vk::DescriptorImageInfo::default()
            .image_view(image.unwrap_or_default())
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.sets[0])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&clips),
            vk::WriteDescriptorSet::default()
                .dst_set(self.sets[0])
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&primitives),
            vk::WriteDescriptorSet::default()
                .dst_set(self.sets[1])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&texture),
        ];
        // SAFETY: Called only after the previous submission completed. Both views
        // remain owned by Renderer throughout the next submitted command buffer.
        let count = if image.is_some() { 3 } else { 2 };
        unsafe {
            self.raw.update_descriptor_sets(&writes[..count], &[]);
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // SAFETY: Renderer waits before drop and drops target framebuffers first.
        // Vulkan destroy accepts null handles for incomplete initialization.
        unsafe {
            for p in self.pipelines {
                self.raw.destroy_pipeline(p, None);
            }
            for l in self.layouts {
                self.raw.destroy_pipeline_layout(l, None);
            }
            self.raw.destroy_descriptor_pool(self.pool, None);
            for l in self.set_layouts {
                self.raw.destroy_descriptor_set_layout(l, None);
            }
            for p in self.passes {
                self.raw.destroy_render_pass(p, None);
            }
        }
    }
}

fn render_pass(
    raw: &ash::Device,
    format: vk::Format,
    final_layout: vk::ImageLayout,
    stage: vk::PipelineStageFlags,
    access: vk::AccessFlags,
) -> Result<vk::RenderPass> {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(final_layout)];
    let colors = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&colors)];
    let dependencies = [
        vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::TOP_OF_PIPE)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE),
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(stage)
            .dst_access_mask(access),
    ];
    // SAFETY: Descriptions are fully initialized and borrowed only for creation.
    Ok(unsafe {
        raw.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies),
            None,
        )?
    })
}

struct Shader<'a>(&'a ash::Device, vk::ShaderModule);
impl<'a> Shader<'a> {
    fn new(raw: &'a ash::Device, bytes: &[u8]) -> Result<Self> {
        // SPIR-V is generated by the build script; decode to aligned host words.
        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        // SAFETY: The compiler-produced word sequence lives through creation.
        Ok(Self(raw, unsafe {
            raw.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)?
        }))
    }
}
impl Drop for Shader<'_> {
    fn drop(&mut self) {
        // SAFETY: Shader modules are no longer needed after pipeline creation.
        unsafe {
            self.0.destroy_shader_module(self.1, None);
        }
    }
}

pub(crate) fn graphics(
    raw: &ash::Device,
    pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    blend: bool,
    vertex: &[u8],
    fragment: &[u8],
    fragment_entry: &std::ffi::CStr,
) -> Result<vk::Pipeline> {
    let vertex = Shader::new(raw, vertex)?;
    let fragment = Shader::new(raw, fragment)?;
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex.1)
            .name(c"vs_main"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment.1)
            .name(fragment_entry),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let samples = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let attachment = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(blend)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let colors = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachment);
    let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
    let infos = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&samples)
        .color_blend_state(&colors)
        .dynamic_state(&dynamic)
        .layout(layout)
        .render_pass(pass)
        .subpass(0)];
    // SAFETY: All layout/pass/module handles belong to raw and outlive this call;
    // dynamic viewport/scissor are set before any draw using the resulting pipeline.
    match unsafe { raw.create_graphics_pipelines(vk::PipelineCache::null(), &infos, None) } {
        Ok(pipelines) => Ok(pipelines[0]),
        Err((partial, error)) => {
            // SAFETY: A failed multi-pipeline call can return owned partial handles.
            unsafe {
                for pipeline in partial {
                    raw.destroy_pipeline(pipeline, None);
                }
            }
            Err(error.into())
        }
    }
}
