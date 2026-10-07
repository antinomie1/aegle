//! Device, queue and the fixed pipelines shared by offscreen and window targets.
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace, Instance,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPipeline, RenderPipelineDescriptor,
    ShaderModule, ShaderStages, TextureFormat, TextureSampleType, TextureViewDimension,
    VertexState,
};

use std::{
    cell::RefCell,
    sync::{Arc, OnceLock},
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
    /// Application textures registered on this device.
    #[cfg(feature = "text")]
    pub external: crate::external::Registry,
    resolve_layout: BindGroupLayout,
    resolve_shader: ShaderModule,
    /// Encoding pipelines by output format; a shared device serves several surfaces.
    resolve: RefCell<Vec<(TextureFormat, RenderPipeline)>>,
    #[cfg_attr(not(feature = "window"), allow(dead_code))]
    pub adapter: wgpu::Adapter,
    /// The first uncaptured wgpu error or device loss, recorded by wgpu's
    /// callbacks instead of its default panic. Once set, no work is accepted.
    fault: Arc<OnceLock<Fault>>,
}

struct Fault {
    lost: bool,
    message: String,
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
        // GL reports the fragment count when the vertex stage has none, so the
        // vertex stage needs its own flag besides the per-stage limit.
        let vertex_storage = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VERTEX_STORAGE);
        if !vertex_storage || adapter.limits().max_storage_buffers_per_shader_stage < 2 {
            return Err(Error::Unsupported("vertex-visible storage buffers"));
        }
        let limits = wgpu::Limits {
            max_storage_buffers_per_shader_stage: 2,
            ..wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("aegle"),
                required_limits: limits,
                ..Default::default()
            }))?;
        let capabilities = surface.map(|surface| surface.get_capabilities(&adapter));
        Ok((Self::new(device, queue, adapter), capabilities))
    }

    fn new(device: Device, queue: Queue, adapter: wgpu::Adapter) -> Self {
        let name = adapter.get_info().name;
        let fault = Arc::new(OnceLock::new());
        device.on_uncaptured_error(Arc::new({
            let fault = fault.clone();
            move |error: wgpu::Error| {
                let _ = fault.set(Fault {
                    lost: false,
                    message: error.to_string(),
                });
            }
        }));
        device.set_device_lost_callback({
            let fault = fault.clone();
            move |reason, message| {
                // Dropping the device reports `Destroyed`; nothing uses it after.
                if reason == wgpu::DeviceLostReason::Unknown {
                    let _ = fault.set(Fault {
                        lost: true,
                        message,
                    });
                }
            }
        });
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
            #[cfg(feature = "text")]
            external: Default::default(),
            resolve_layout,
            resolve_shader,
            resolve: RefCell::new(Vec::new()),
            adapter,
            fault,
        }
    }

    /// Refuses work after wgpu reported an error or lost the device. Neither is
    /// recovered: the caller creates a new renderer.
    pub fn check(&self) -> Result {
        match self.fault.get() {
            None => Ok(()),
            Some(Fault {
                lost: true,
                message,
            }) => Err(Error::DeviceLost(message.clone())),
            Some(Fault { message, .. }) => Err(Error::Gpu(message.clone())),
        }
    }

    pub fn resolve_layout(&self) -> &BindGroupLayout {
        &self.resolve_layout
    }

    /// The pipeline encoding the linear image into `format` as premultiplied sRGB,
    /// built once per distinct output format.
    pub fn resolve(&self, format: TextureFormat) -> RenderPipeline {
        let mut cache = self.resolve.borrow_mut();
        if let Some((_, pipeline)) = cache.iter().find(|(have, _)| *have == format) {
            return pipeline.clone();
        }
        let pipeline = pipeline(
            &self.device,
            &self.resolve_shader,
            "fs_main",
            &[Some(&self.resolve_layout)],
            format,
            // The shader writes final bytes; blending would re-encode them.
            BlendState::REPLACE,
        );
        cache.push((format, pipeline.clone()));
        pipeline
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
