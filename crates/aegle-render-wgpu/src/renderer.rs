use aegle_gpu::{Primitive, Recording};
use aegle_scene::Color;
use aegle_types::color_math::linear_rgba;
use std::rc::Rc;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages,
    Extent3d, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
};

use crate::{
    Error, Result,
    gpu::{Gpu, LINEAR},
};

const OFFSCREEN: TextureFormat = TextureFormat::Rgba8Unorm;

/// Fixed configuration.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Square atlas page extent, including one transparent pixel around every
    /// entry. Mask and color pages are allocated when first needed; larger images
    /// and path masks get exact-size textures instead. Default 1024; must be at
    /// least 3 and at most the device's texture dimension limit.
    #[cfg(feature = "text")]
    pub atlas_size: u32,
    /// Window only: keep premultiplied window alpha when the compositor offers it.
    /// Otherwise windows are opaque and require an opaque clear color.
    #[cfg(feature = "window")]
    pub transparent: bool,
}
// Derivable only without `text`; a derived zero-sized page would reject every glyph.
#[allow(clippy::derivable_impls)]
impl Default for Options {
    fn default() -> Self {
        Self {
            #[cfg(feature = "text")]
            atlas_size: 1024,
            #[cfg(feature = "window")]
            transparent: false,
        }
    }
}

/// Which pipeline and texture draws a run of primitives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Geometry,
    #[cfg(feature = "text")]
    Atlas(crate::atlas::Slot),
}

impl Kind {
    fn of(primitive: &Primitive) -> Self {
        match primitive.header[1] {
            0 => Self::Geometry,
            #[cfg(feature = "text")]
            _ => Self::Atlas(crate::atlas::Slot::from_page(primitive.header[2])),
            #[cfg(not(feature = "text"))]
            _ => unreachable!("atlas draws require the text feature"),
        }
    }
}

struct Target {
    size: [u32; 2],
    linear: TextureView,
    resolve: BindGroup,
    /// Offscreen renderers resolve into this texture for explicit readback.
    output: Option<Texture>,
}

#[derive(Default)]
struct Buffers {
    primitives: Option<Buffer>,
    clips: Option<Buffer>,
    group: Option<BindGroup>,
}

/// Portable GPU renderer for retained scenes, independent of windows, UI trees
/// and shaping.
///
/// Geometry is rasterized and blended in a linear RGBA16F image on the GPU; a
/// second pass encodes premultiplied sRGB into the output. Only small draw/clip
/// records and glyph patches are uploaded. Nothing runs between frames.
pub struct Renderer {
    pub(crate) gpu: Rc<Gpu>,
    target: Option<Target>,
    buffers: Buffers,
    pub(crate) rec: Recording,
    /// Adjacent primitives sharing a pipeline and texture: kind and instance range.
    batches: Vec<(Kind, u32, u32)>,
    #[cfg(feature = "text")]
    pub(crate) atlas: crate::atlas::Atlas,
    /// Application textures referenced by the records since the last submission.
    #[cfg(feature = "text")]
    pub(crate) frame_textures: Vec<BindGroup>,
    pub(crate) size: [u32; 2],
    /// Shader viewport parameters; the negative height selects WebGPU clip space.
    pub(crate) viewport: [f32; 2],
    clear: [f64; 4],
    /// True once a pass has cleared the linear image this frame.
    loaded: bool,
}

impl Renderer {
    /// Creates a headless device on the best available backend. `WGPU_BACKEND`,
    /// `WGPU_ADAPTER_NAME` and `WGPU_POWER_PREF` select a specific one.
    pub fn new(options: Options) -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        Self::with_gpu(Rc::new(Gpu::connect(&instance, None)?.0), options)
    }

    pub(crate) fn with_gpu(gpu: Rc<Gpu>, options: Options) -> Result<Self> {
        // A page holds at least one pixel inside its transparent border.
        #[cfg(feature = "text")]
        if options.atlas_size < 3
            || options.atlas_size > gpu.device.limits().max_texture_dimension_2d
        {
            return Err(Error::InvalidSize);
        }
        #[cfg(not(feature = "text"))]
        let _ = options;
        Ok(Self {
            gpu,
            target: None,
            buffers: Buffers::default(),
            rec: Recording::default(),
            batches: Vec::new(),
            #[cfg(feature = "text")]
            atlas: crate::atlas::Atlas::new(options.atlas_size),
            #[cfg(feature = "text")]
            frame_textures: Vec::new(),
            size: [0; 2],
            viewport: [0.0; 2],
            clear: [0.0; 4],
            loaded: false,
        })
    }

    /// Name reported by the selected adapter (including CPU implementations).
    pub fn device_name(&self) -> &str {
        &self.gpu.name
    }

    /// Glyphs, images and path masks currently resident on the GPU. Exact-size
    /// textures stay for one frame after their last use.
    #[cfg(feature = "text")]
    pub fn resident_entries(&self) -> usize {
        self.atlas.len()
    }

    /// Begins a complete offscreen frame. Same-sized targets are reused.
    pub fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        clear: Color,
    ) -> Result<crate::Frame<'_>> {
        self.start(width, height, clear, true)?;
        Ok(crate::Frame::new(
            self,
            crate::frame::Presentation::Offscreen,
        ))
    }

    pub(crate) fn start(
        &mut self,
        width: u32,
        height: u32,
        clear: Color,
        offscreen: bool,
    ) -> Result {
        self.gpu.check()?;
        let limit = self.gpu.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > limit || height > limit {
            return Err(Error::InvalidSize);
        }
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.size != [width, height] || t.output.is_some() != offscreen)
        {
            self.target = None;
            self.target = Some(self.create_target(width, height, offscreen));
        }
        #[cfg(feature = "text")]
        self.atlas.begin_frame();
        self.size = [width, height];
        self.viewport = aegle_gpu::viewport(width, height, true);
        self.clear = linear_rgba(clear.to_rgba()).map(f64::from);
        self.loaded = false;
        self.rec.clear();
        Ok(())
    }

    fn create_target(&self, width: u32, height: u32, offscreen: bool) -> Target {
        let texture = |format, usage| {
            self.gpu.device.create_texture(&TextureDescriptor {
                label: Some("aegle target"),
                size: Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let linear = texture(
            LINEAR,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let resolve = self.gpu.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: self.gpu.resolve_layout(),
            entries: &[BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&linear),
            }],
        });
        Target {
            size: [width, height],
            linear,
            resolve,
            output: offscreen.then(|| {
                texture(
                    OFFSCREEN,
                    TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                )
            }),
        }
    }

    /// Writes this flush's rows, growing the storage buffers (never shrinking)
    /// and rebuilding the group only when a buffer was replaced.
    fn upload(&mut self) {
        let device = &self.gpu.device;
        let mut grown = false;
        let mut write = |slot: &mut Option<Buffer>, bytes: &[u8]| {
            let need = (bytes.len() as u64).next_power_of_two().max(4096);
            if slot.as_ref().is_none_or(|b| b.size() < need) {
                *slot = Some(device.create_buffer(&BufferDescriptor {
                    label: Some("aegle records"),
                    size: need,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                grown = true;
            }
            if !bytes.is_empty() {
                self.gpu
                    .queue
                    .write_buffer(slot.as_ref().unwrap(), 0, bytes);
            }
        };
        write(
            &mut self.buffers.primitives,
            bytemuck::cast_slice(&self.rec.primitives),
        );
        write(
            &mut self.buffers.clips,
            bytemuck::cast_slice(&self.rec.clips),
        );
        if grown {
            self.buffers.group = Some(
                device.create_bind_group(&BindGroupDescriptor {
                    label: Some("aegle records"),
                    layout: &self.gpu.records,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: self.buffers.clips.as_ref().unwrap().as_entire_binding(),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: self
                                .buffers
                                .primitives
                                .as_ref()
                                .unwrap()
                                .as_entire_binding(),
                        },
                    ],
                }),
            );
        }
    }

    /// Submits the recorded primitives. With `output`, also encodes the linear
    /// image into it. Queue ordering keeps later buffer and atlas writes after
    /// this submission, so a split frame never sees its own later data.
    pub(crate) fn flush(&mut self, output: Option<&Texture>) -> Result {
        if self.rec.primitives.is_empty() && output.is_none() {
            return Ok(());
        }
        self.batches.clear();
        for (index, primitive) in self.rec.primitives.iter().enumerate() {
            let (kind, index) = (Kind::of(primitive), index as u32);
            match self.batches.last_mut() {
                Some((last, _, end)) if *last == kind => *end = index + 1,
                _ => self.batches.push((kind, index, index + 1)),
            }
        }
        if !self.rec.primitives.is_empty() {
            self.upload();
        }
        let resolve = output.map(|texture| self.gpu.resolve(texture.format()));
        let target = self.target.as_ref().unwrap();
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        {
            let [r, g, b, a] = self.clear;
            let load = if self.loaded {
                LoadOp::Load
            } else {
                LoadOp::Clear(wgpu::Color { r, g, b, a })
            };
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("aegle scene"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target.linear,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load,
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if let Some(group) = &self.buffers.group {
                pass.set_bind_group(0, group, &[]);
            }
            for &(kind, start, end) in &self.batches {
                self.bind(&mut pass, kind);
                pass.draw(0..6, start..end);
            }
        }
        if let Some(texture) = output {
            let view = texture.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("aegle resolve"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(resolve.as_ref().expect("resolve for output"));
            pass.set_bind_group(0, &target.resolve, &[]);
            pass.draw(0..3, 0..1);
        }
        self.gpu.queue.submit([encoder.finish()]);
        // wgpu validates encoding and submission synchronously.
        self.gpu.check()?;
        self.loaded = true;
        self.rec.primitives.clear();
        #[cfg(feature = "text")]
        self.frame_textures.clear();
        Ok(())
    }

    fn bind(&self, pass: &mut wgpu::RenderPass<'_>, kind: Kind) {
        match kind {
            Kind::Geometry => pass.set_pipeline(&self.gpu.geometry),
            #[cfg(feature = "text")]
            Kind::Atlas(slot) => {
                pass.set_pipeline(&self.gpu.text);
                let group = match slot {
                    crate::atlas::Slot::External(index) => &self.frame_textures[index as usize],
                    slot => self.atlas.group(slot),
                };
                pass.set_bind_group(1, group, &[]);
            }
        }
    }

    /// Completes the frame into `window`, or the offscreen texture when absent.
    pub(crate) fn finish(&mut self, window: Option<&Texture>) -> Result {
        let output = match window {
            Some(texture) => texture.clone(),
            None => self.target.as_ref().unwrap().output.clone().unwrap(),
        };
        self.flush(Some(&output))
    }

    /// Copies the last finished offscreen frame as tightly packed, top-down,
    /// premultiplied sRGB RGBA8. `out` must be exactly `width × height × 4` bytes.
    /// This waits for the GPU; normal drawing never reads back.
    pub fn read_pixels(&mut self, out: &mut [u8]) -> Result {
        let [width, height] = self.size;
        let texture = self
            .target
            .as_ref()
            .and_then(|t| t.output.as_ref())
            .ok_or(Error::InvalidState("no completed offscreen frame"))?;
        if out.len() != width as usize * height as usize * 4 {
            return Err(Error::InvalidSize);
        }
        let row = width as usize * 4;
        let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        let staging = self.gpu.device.create_buffer(&BufferDescriptor {
            label: Some("aegle readback"),
            size: (padded * height as usize) as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: None,
                },
            },
            texture.size(),
        );
        self.gpu.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| Error::Gpu(error.to_string()))?;
        self.gpu.check()?;
        receiver
            .recv()
            .map_err(|_| Error::InvalidState("readback callback dropped"))?
            .map_err(Error::Readback)?;
        let mapped = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|_| Error::InvalidState("readback range unavailable"))?;
        for (dest, source) in out.chunks_exact_mut(row).zip(mapped.chunks_exact(padded)) {
            dest.copy_from_slice(&source[..row]);
        }
        Ok(())
    }
}
