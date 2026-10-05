use crate::{
    Error, Result,
    commands::Commands,
    device::Device,
    geometry::{Clip, Recording},
    memory::Buffer,
    pipeline::Pipeline,
    target::Target,
};
use aegle_scene::{Affine, Color, Rect, Scene};
use ash::vk;

/// Fixed configuration; budgets are independent of hidden driver allocations.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Vulkan enumeration index, or prefer a suitable hardware device when absent.
    pub device_index: Option<u32>,
    /// Bytes of explicit VkDeviceMemory allocations, including alignment and readback.
    pub memory_budget: u64,
    /// Bytes of CPU draw/clip vector capacity; excludes driver command storage.
    pub recording_budget: usize,
    /// Independent glyph raster, atlas and upload limits; enabled only by `text`.
    #[cfg(feature = "text")]
    pub text: crate::TextOptions,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            device_index: None,
            memory_budget: 16 * 1024 * 1024,
            recording_budget: 1024 * 1024,
            #[cfg(feature = "text")]
            text: crate::TextOptions::default(),
        }
    }
}

/// Explicit retained allocations; not complete driver or process memory usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Sum of bound VkDeviceMemory allocation sizes owned by this renderer.
    pub device_bytes: u64,
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
    target: Option<Target>,
    clips: Option<Buffer>,
    readback: Option<Buffer>,
    #[cfg(feature = "text")]
    text: crate::text::Text,
    pipeline: Pipeline,
    commands: Commands,
    device: Device,
    recording: Recording,
    options: Options,
    name: String,
    busy: bool,
    image_ready: bool,
}

impl Renderer {
    /// Loads the installed Vulkan loader and creates a Vulkan 1.1 graphics device.
    /// No image, upload buffer or readback allocation is made until first use.
    pub fn new(options: Options) -> Result<Self> {
        let device = Device::new(options.device_index)?;
        let name = std::ffi::CStr::from_bytes_until_nul(bytemuck::cast_slice(
            &device.properties.device_name,
        ))
        .expect("Vulkan guarantees a terminated physical device name")
        .to_string_lossy()
        .into_owned();
        let pipeline = Pipeline::new(&device)?;
        let commands = Commands::new(&device)?;
        #[cfg(feature = "text")]
        let text = crate::text::Text::new(&device, &pipeline, options.text)?;
        Ok(Self {
            target: None,
            clips: None,
            readback: None,
            #[cfg(feature = "text")]
            text,
            pipeline,
            commands,
            device,
            recording: Recording::default(),
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
            recording_bytes: self.recording.draws.capacity() * size_of::<crate::geometry::Draw>()
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
        self.target = None;
        self.clips = None;
        self.readback = None;
        self.recording = Recording::default();
        #[cfg(feature = "text")]
        self.text.atlas.clear();
        self.image_ready = false;
        Ok(())
    }

    fn remaining(&self) -> u64 {
        self.options.memory_budget - self.stats().device_bytes
    }

    fn base_bytes(&self) -> u64 {
        self.target.as_ref().map_or(0, Target::bytes)
            + self.clips.as_ref().map_or(0, |b| b.allocation)
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
        let bytes = (self.recording.clips.len().max(1) * size_of::<Clip>()) as u64;
        if self.clips.as_ref().is_none_or(|b| b.size < bytes) {
            self.clips = None;
            self.clips = Some(Buffer::new(
                &self.device,
                bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
                self.remaining(),
            )?);
        }
        let clips = self.clips.as_mut().unwrap();
        if !self.recording.clips.is_empty() {
            clips.write(0, bytemuck::cast_slice(&self.recording.clips))?;
        }
        #[cfg(feature = "text")]
        self.text
            .atlas
            .prepare_upload(&self.device, self.options.memory_budget - self.base_bytes())?;
        let target = self.target.as_ref().unwrap();
        self.pipeline.update(
            self.clips.as_ref().unwrap().handle,
            bytes,
            target.linear.view,
        );
        self.commands.begin()?;
        #[cfg(feature = "text")]
        self.text
            .atlas
            .record_uploads(&self.device.raw, self.commands.buffer);
        self.commands.render(
            target,
            &self.pipeline,
            &self.recording,
            aegle_types::color_math::linear_rgba(clear.to_rgba()),
            #[cfg(feature = "text")]
            &self.text.pipeline,
        );
        self.commands.submit(self.device.queue)?;
        #[cfg(feature = "text")]
        self.text.atlas.commit();
        self.busy = true;
        self.image_ready = true;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // A lost device may fail waiting; Vulkan still permits object destruction.
        let _ = self.wait();
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
        #[cfg(feature = "text")]
        let text_budget = self.renderer.options.memory_budget - self.renderer.base_bytes();
        let result = self.renderer.recording.append(
            scene,
            transform,
            clip,
            target.width,
            target.height,
            self.renderer.options.recording_budget,
            #[cfg(feature = "text")]
            |recording, run, state| {
                self.renderer.text.record(
                    &self.renderer.device,
                    recording,
                    run,
                    state,
                    target.width,
                    target.height,
                    text_budget,
                    self.renderer.options.recording_budget,
                )
            },
        );
        self.failed |= result.is_err();
        result
    }
    /// Submits graphics and color encoding, without waiting or copying pixels to CPU.
    /// The next frame/readback waits on this submission's fence before reusing memory.
    pub fn finish(self) -> Result {
        if self.failed {
            return Err(Error::FrameFailed);
        }
        self.renderer.submit(self.clear)
    }
}
