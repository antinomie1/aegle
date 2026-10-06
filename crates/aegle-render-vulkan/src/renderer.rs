use crate::{
    Error, Result, commands::Commands, device::Device, memory::Buffer, pipeline::Pipeline,
    target::Target,
};
use aegle_gpu::{Clip, Primitive, Recording, Step, Walker, viewport};
use aegle_scene::{Affine, Color, Rect, Scene};
use ash::vk;
use std::rc::Rc;

/// Fixed configuration; budgets are independent of hidden driver allocations.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Vulkan enumeration index. When absent, prefers a suitable integrated GPU,
    /// then discrete, virtual and finally CPU devices.
    pub device_index: Option<u32>,
    /// Bytes of explicit VkDeviceMemory allocations, including alignment/readback,
    /// plus the opaque swapchain image estimate when presenting to a window.
    pub memory_budget: u64,
    /// Bytes of CPU draw/clip vector capacity; excludes driver command storage.
    pub recording_budget: usize,
    /// Independent glyph raster, atlas and upload limits; enabled only by `text`.
    #[cfg(feature = "text")]
    pub text: crate::TextOptions,
    /// Window only: keep premultiplied window alpha. This adds the RGBA16F
    /// working image and encoding pass. Otherwise a window renders directly into
    /// an sRGB swapchain image when the surface offers one, and needs opaque clears.
    #[cfg(feature = "window")]
    pub transparent: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            device_index: None,
            memory_budget: 16 * 1024 * 1024,
            recording_budget: 1024 * 1024,
            #[cfg(feature = "text")]
            text: crate::TextOptions::default(),
            #[cfg(feature = "window")]
            transparent: false,
        }
    }
}

/// Explicit retained allocations; not complete driver or process memory usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Sum of bound VkDeviceMemory allocation sizes owned by this renderer.
    pub device_bytes: u64,
    /// Estimated swapchain image bytes (width × height × 4 × image count), separate
    /// from device_bytes because WSI allocations are opaque to Vulkan applications.
    pub swapchain_bytes: u64,
    /// Capacity bytes of CPU primitive and clipping vectors.
    pub recording_bytes: usize,
}

/// Independent Vulkan renderer, without platform, shaping or UI dependencies.
///
/// Geometry is rasterized and blended by Vulkan graphics pipelines. Only small
/// draw/clip records and optional glyph patches are uploaded; no CPU-rasterized
/// framebuffer is uploaded.
/// All access is serialized through mutable methods. No idle render loop is owned.
pub struct Renderer {
    // Declaration order destroys children before their render passes and device.
    pub(crate) target: Option<Target>,
    #[cfg(feature = "window")]
    pub(crate) swapchain: Option<crate::swapchain::Swapchain>,
    #[cfg(feature = "window")]
    pub(crate) window_size: [u32; 2],
    /// This window's surface; it outlives the swapchain and precedes the device.
    #[cfg(feature = "window")]
    pub(crate) surface: Option<crate::surface::Surface>,
    /// Host-visible clip and primitive storage, rewritten after each fence.
    buffers: [Option<Buffer>; 2],
    readback: Option<Buffer>,
    #[cfg(feature = "text")]
    text: crate::text::Text,
    pub(crate) pipeline: Pipeline,
    commands: Commands,
    pub(crate) device: Rc<Device>,
    recording: Recording,
    pub(crate) options: Options,
    name: String,
    busy: bool,
    image_ready: bool,
}

impl Renderer {
    /// Loads the installed Vulkan loader and creates a Vulkan 1.1 graphics device.
    /// No image, upload buffer or readback allocation is made until first use.
    pub fn new(options: Options) -> Result<Self> {
        Self::with_device(
            options,
            Rc::new(Device::new(options.device_index)?),
            #[cfg(feature = "window")]
            None,
        )
    }

    pub(crate) fn with_device(
        options: Options,
        device: Rc<Device>,
        #[cfg(feature = "window")] surface: Option<crate::surface::Surface>,
    ) -> Result<Self> {
        let name = std::ffi::CStr::from_bytes_until_nul(bytemuck::cast_slice(
            &device.properties.device_name,
        ))
        .expect("Vulkan guarantees a terminated physical device name")
        .to_string_lossy()
        .into_owned();
        #[cfg(feature = "window")]
        let pipeline = Pipeline::new(&device, surface.as_ref(), options.transparent)?;
        #[cfg(not(feature = "window"))]
        let pipeline = Pipeline::new(&device)?;
        let commands = Commands::new(&device)?;
        #[cfg(feature = "text")]
        let text = crate::text::Text::new(&device, &pipeline, options.text)?;
        Ok(Self {
            target: None,
            #[cfg(feature = "window")]
            swapchain: None,
            #[cfg(feature = "window")]
            window_size: [0; 2],
            #[cfg(feature = "window")]
            surface,
            buffers: [None, None],
            readback: None,
            #[cfg(feature = "text")]
            text,
            pipeline,
            commands,
            device,
            recording: Recording::with_limit(options.recording_budget),
            options,
            name,
            busy: false,
            image_ready: false,
        })
    }

    /// Name reported by the selected physical device (including CPU drivers).
    pub fn device_name(&self) -> &str {
        &self.name
    }

    /// Begins a complete frame. Extents must be nonzero and within device limits.
    /// Waits for the preceding submission, then reuses same-sized attachments.
    /// Resizing releases old attachments/readback before allocating replacements;
    /// a resize failure therefore invalidates the previous image.
    pub fn begin_frame(&mut self, width: u32, height: u32, clear: Color) -> Result<Frame<'_>> {
        let limits = self.device.properties.limits;
        if width == 0
            || height == 0
            || width > limits.max_image_dimension2_d
            || height > limits.max_image_dimension2_d
            || width > limits.max_framebuffer_width
            || height > limits.max_framebuffer_height
            || width > limits.max_viewport_dimensions[0]
            || height > limits.max_viewport_dimensions[1]
            || width as f32 > limits.viewport_bounds_range[1]
            || height as f32 > limits.viewport_bounds_range[1]
        {
            return Err(Error::InvalidSize);
        }
        self.wait()?;
        self.image_ready = false;
        self.recording.clear();
        #[cfg(feature = "text")]
        self.text.atlas.begin_frame();
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.width != width || t.height != height)
        {
            self.target = None;
            self.readback = None;
            self.target = Some(Target::new(
                &self.device,
                &self.pipeline,
                width,
                height,
                self.remaining(),
            )?);
        }
        Ok(Frame {
            renderer: self,
            clear,
            failed: false,
        })
    }

    /// Waits for the last submitted frame or readback, without polling a timer.
    pub fn wait(&mut self) -> Result {
        if self.busy {
            self.commands.wait()?;
            self.busy = false;
            #[cfg(feature = "text")]
            self.text.atlas.retire_upload();
        }
        Ok(())
    }

    /// Reads the latest successfully submitted image into exactly width×height×4 bytes.
    /// Rows are tightly packed, top to bottom, premultiplied sRGB RGBA8. Allocates a
    /// reusable host-visible transfer buffer on first readback, within the same budget.
    pub fn read_pixels(&mut self, pixels: &mut [u8]) -> Result {
        if !self.image_ready {
            return Err(Error::InvalidState("no completed Vulkan frame"));
        }
        let target = self.target.as_ref().unwrap();
        let bytes = u64::from(target.width) * u64::from(target.height) * 4;
        if pixels.len() as u64 != bytes {
            return Err(Error::InvalidSize);
        }
        self.wait()?;
        if self.readback.is_none() {
            self.readback = Some(Buffer::new(
                &self.device,
                bytes,
                vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
                self.remaining(),
            )?);
        }
        self.commands.begin()?;
        self.commands.copy(
            self.target.as_ref().unwrap(),
            self.readback.as_ref().unwrap().handle,
            bytes,
        );
        self.commands.submit(self.device.queue)?;
        self.busy = true;
        self.wait()?;
        self.readback.as_mut().unwrap().read(0, pixels)
    }

    /// Current explicit memory usage; excludes pipelines, command pools and driver data.
    pub fn stats(&self) -> Stats {
        Stats {
            device_bytes: self.base_bytes() + self.text_bytes(),
            swapchain_bytes: self.swapchain_bytes(),
            recording_bytes: self.recording.primitives.capacity() * size_of::<Primitive>()
                + self.recording.clips.capacity() * size_of::<Clip>(),
        }
    }

    /// Glyph residency, upload storage and CPU cache use under independent limits.
    #[cfg(feature = "text")]
    pub fn text_stats(&self) -> crate::TextStats {
        self.text.atlas.stats()
    }

    /// Waits and releases images, transfers and CPU records, including optional
    /// glyph caches/atlases. Pipeline objects remain reusable. Reading pixels
    /// requires a new completed frame.
    pub fn release_images(&mut self) -> Result {
        self.wait()?;
        #[cfg(feature = "window")]
        self.release_swapchain()?;
        self.target = None;
        self.buffers = [None, None];
        self.readback = None;
        self.recording = Recording::with_limit(self.options.recording_budget);
        #[cfg(feature = "text")]
        self.text.atlas.clear();
        self.image_ready = false;
        Ok(())
    }

    pub(crate) fn remaining(&self) -> u64 {
        self.options.memory_budget - self.stats().device_bytes - self.swapchain_bytes()
    }

    fn swapchain_bytes(&self) -> u64 {
        #[cfg(feature = "window")]
        {
            self.swapchain.as_ref().map_or(0, |chain| chain.bytes)
        }
        #[cfg(not(feature = "window"))]
        {
            0
        }
    }

    fn base_bytes(&self) -> u64 {
        self.target.as_ref().map_or(0, Target::bytes)
            + self
                .buffers
                .iter()
                .flatten()
                .map(|b| b.allocation)
                .sum::<u64>()
            + self.readback.as_ref().map_or(0, |b| b.allocation)
    }

    fn text_bytes(&self) -> u64 {
        #[cfg(feature = "text")]
        {
            self.text.atlas.device_bytes()
        }
        #[cfg(not(feature = "text"))]
        {
            0
        }
    }

    fn submit(&mut self, clear: Color) -> Result {
        for index in 0..2 {
            let data: &[u8] = if index == 0 {
                bytemuck::cast_slice(&self.recording.clips)
            } else {
                bytemuck::cast_slice(&self.recording.primitives)
            };
            // Storage bindings must be nonempty even for a clear-only frame.
            let bytes = data
                .len()
                .max(size_of::<Primitive>().max(size_of::<Clip>())) as u64;
            if self.buffers[index].as_ref().is_none_or(|b| b.size < bytes) {
                self.buffers[index] = None;
                self.buffers[index] = Some(Buffer::new(
                    &self.device,
                    bytes,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                    vk::MemoryPropertyFlags::HOST_VISIBLE,
                    self.remaining(),
                )?);
            }
            if !data.is_empty() {
                self.buffers[index].as_mut().unwrap().write(0, data)?;
            }
        }
        #[cfg(feature = "text")]
        self.text.atlas.prepare_upload(
            &self.device,
            self.options.memory_budget - self.base_bytes() - self.swapchain_bytes(),
        )?;
        let target = self.target.as_ref().unwrap();
        let [clips, primitives] = &self.buffers;
        self.pipeline.update(
            clips.as_ref().unwrap().handle,
            primitives.as_ref().unwrap().handle,
            target.linear.as_ref().map(|image| image.view),
        );
        self.commands.begin()?;
        #[cfg(feature = "text")]
        self.text
            .atlas
            .record_uploads(&self.device.raw, self.commands.buffer);
        let output = target.frames[1];
        #[cfg(feature = "window")]
        let acquired = self
            .swapchain
            .as_mut()
            .map(|chain| chain.acquire())
            .transpose()?;
        #[cfg(feature = "window")]
        let output = if let Some((index, _, _)) = acquired {
            self.swapchain.as_ref().unwrap().frames[index as usize]
        } else {
            output
        };
        self.commands.render(
            target,
            &self.pipeline,
            &self.recording,
            aegle_types::color_math::linear_rgba(clear.to_rgba()),
            output,
            #[cfg(feature = "text")]
            &self.text.pipeline,
        );
        let signals: &[vk::Semaphore] = &[];
        #[cfg(feature = "window")]
        let signal;
        #[cfg(feature = "window")]
        let signals = if let Some((_, semaphore, _)) = acquired {
            signal = [semaphore];
            &signal[..]
        } else {
            signals
        };
        self.commands.submit_signal(self.device.queue, signals)?;
        #[cfg(feature = "text")]
        self.text.atlas.commit();
        self.busy = true;
        self.image_ready = true;
        #[cfg(feature = "window")]
        if let Some((index, _, suboptimal)) = acquired {
            self.swapchain
                .as_mut()
                .unwrap()
                .present(self.device.queue, index, suboptimal)?;
        }
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // A lost device may fail waiting; Vulkan still permits object destruction.
        let _ = self.wait();
        #[cfg(feature = "window")]
        let _ = self.release_swapchain();
    }
}

/// A frame recorded on the CPU, borrowing its renderer exclusively.
/// Dropping it discards its records without submitting. A failed draw poisons the
/// frame so partially recorded content cannot accidentally be presented.
pub struct Frame<'a> {
    renderer: &'a mut Renderer,
    clear: Color,
    failed: bool,
}
impl Frame<'_> {
    /// Actual physical extent of this frame, including native surface constraints.
    pub fn extent(&self) -> [u32; 2] {
        let target = self.renderer.target.as_ref().unwrap();
        [target.width, target.height]
    }
    /// Appends a retained scene with a logical-to-device transform.
    pub fn draw(&mut self, scene: &Scene, transform: Affine) -> Result {
        self.draw_clipped(scene, transform, None)
    }
    /// Appends a scene intersected with an optional device-space clip. The clip
    /// is not transformed again and consumes one of the eight available clip layers.
    pub fn draw_clipped(&mut self, scene: &Scene, transform: Affine, clip: Option<Rect>) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        let target = self.renderer.target.as_ref().unwrap();
        let extent = [target.width, target.height];
        #[cfg(feature = "text")]
        let text_budget = self.renderer.options.memory_budget
            - self.renderer.base_bytes()
            - self.renderer.swapchain_bytes();
        let renderer = &mut *self.renderer;
        let view = viewport(extent[0], extent[1], false);
        let result = (|| -> Result {
            let mut walker = Walker::new(
                scene,
                transform,
                clip,
                extent,
                view,
                &mut renderer.recording,
            )?;
            loop {
                match walker.step(&mut renderer.recording)? {
                    Step::Done => return Ok(()),
                    Step::Recorded => {}
                    #[cfg(feature = "text")]
                    Step::Command(command, state) => renderer.text.record(
                        &renderer.device,
                        &mut renderer.recording,
                        scene,
                        command,
                        state,
                        crate::text::Limits {
                            viewport: view,
                            device: text_budget,
                        },
                    )?,
                    #[cfg(not(feature = "text"))]
                    Step::Command(..) => return Err(Error::UnsupportedCommand),
                }
            }
        })();
        self.failed |= result.is_err();
        result
    }
    /// Submits graphics and color encoding without copying pixels to CPU. A native
    /// window frame also acquires a FIFO image (which may block) and presents it.
    /// The next frame/readback waits on the submission fence before reusing memory.
    pub fn finish(self) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        self.renderer.submit(self.clear)
    }
}
