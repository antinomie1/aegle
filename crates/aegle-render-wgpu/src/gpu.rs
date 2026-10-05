//! Device, queue and the fixed pipelines shared by offscreen and window targets.
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace, Instance,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPipeline, RenderPipelineDescriptor,
    ShaderModule, ShaderStages, TextureFormat, TextureSampleType, TextureViewDimension,
    VertexState,
};

use crate::{Error, Result};

/// Linear working format: blending happens here, never in sRGB.
pub(crate) const LINEAR: TextureFormat = TextureFormat::Rgba16Float;

pub(crate) struct Gpu {
    pub device: Device,
    pub queue: Queue,
    pub name: String,
    /// Group 0: clip and primitive storage rows.
    pub records: BindGroupLayout,
    pub geometry: RenderPipeline,
    #[cfg(feature = "text")]
    pub pages: BindGroupLayout,
    #[cfg(feature = "text")]
    pub text: RenderPipeline,
    #[cfg(feature = "text")]
    pub sampler: wgpu::Sampler,
    resolve_layout: BindGroupLayout,
    resolve_shader: ShaderModule,
    resolve: Option<(TextureFormat, RenderPipeline)>,
}

fn wgsl<'a>(label: &'a str, source: &'a str) -> wgpu::ShaderModuleDescriptor<'a> {
    wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    }
}

fn layout_entry(binding: u32, visibility: ShaderStages, ty: BindingType) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}

impl Gpu {
    /// Picks an adapter (honoring `WGPU_BACKEND` and `WGPU_ADAPTER_NAME`), and
    /// requests the smallest device that can run the fixed pipelines.
    pub fn connect(
        instance: &Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<(Self, Option<wgpu::SurfaceCapabilities>)> {
        let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(
            instance, surface,
        ))?;
        let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
        if limits.max_storage_buffers_per_shader_stage < 2 {
            return Err(Error::Unsupported("vertex-visible storage buffers"));
        }
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("aegle"),
                required_limits: limits,
                ..Default::default()
            }))?;
        let capabilities = surface.map(|surface| surface.get_capabilities(&adapter));
        Ok((
            Self::new(device, queue, adapter.get_info().name),
            capabilities,
        ))
    }

    fn new(device: Device, queue: Queue, name: String) -> Self {
        let storage = |binding| {
            layout_entry(
                binding,
                ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )
        };
        let records = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("aegle records"),
            entries: &[storage(0), storage(1)],
        });
        let shader = device.create_shader_module(wgsl("geometry", aegle_gpu::GEOMETRY_WGSL));
        let geometry = pipeline(
            &device,
            &shader,
            "fs_main",
            &[Some(&records)],
            LINEAR,
            BlendState::PREMULTIPLIED_ALPHA_BLENDING,
        );
        #[cfg(feature = "text")]
        let (pages, text) = {
            let pages = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("aegle glyph page"),
                entries: &[
                    layout_entry(
                        0,
                        ShaderStages::FRAGMENT,
                        BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: true },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                    ),
                    layout_entry(
                        1,
                        ShaderStages::FRAGMENT,
                        BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    ),
                ],
            });
            let text = pipeline(
                &device,
                &shader,
                "fs_text",
                &[Some(&records), Some(&pages)],
                LINEAR,
                BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            );
            (pages, text)
        };
        #[cfg(feature = "text")]
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let resolve_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("aegle resolve"),
            entries: &[layout_entry(
                0,
                ShaderStages::FRAGMENT,
                BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
            )],
        });
        let resolve_shader = device.create_shader_module(wgsl("resolve", aegle_gpu::RESOLVE_WGSL));
        Self {
            device,
            queue,
            name,
            records,
            geometry,
            #[cfg(feature = "text")]
            pages,
            #[cfg(feature = "text")]
            text,
            #[cfg(feature = "text")]
            sampler,
            resolve_layout,
            resolve_shader,
            resolve: None,
        }
    }

    pub fn resolve_layout(&self) -> &BindGroupLayout {
        &self.resolve_layout
    }

    /// Builds the pipeline encoding the linear image into `format` as premultiplied
    /// sRGB, once per distinct output format.
    pub fn prepare_resolve(&mut self, format: TextureFormat) {
        if self
            .resolve
            .as_ref()
            .is_none_or(|(have, _)| *have != format)
        {
            let pipeline = pipeline(
                &self.device,
                &self.resolve_shader,
                "fs_main",
                &[Some(&self.resolve_layout)],
                format,
                // The shader writes final bytes; blending would re-encode them.
                BlendState::REPLACE,
            );
            self.resolve = Some((format, pipeline));
        }
    }

    /// The pipeline selected by the last [`Self::prepare_resolve`].
    pub fn resolve(&self) -> &RenderPipeline {
        &self.resolve.as_ref().unwrap().1
    }
}

fn pipeline(
    device: &Device,
    shader: &ShaderModule,
    fragment: &str,
    layouts: &[Option<&BindGroupLayout>],
    format: TextureFormat,
    blend: BlendState,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(&layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: PrimitiveState {
            front_face: FrontFace::Ccw,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
