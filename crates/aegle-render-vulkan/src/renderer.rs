use crate::{
    Error, Frame, Result, commands::Commands, device::Device, memory::Buffer, pipeline::Pipeline,
    target::Target,
};
use aegle_gpu::{Clip, Primitive, Recording};
use aegle_scene::Color;
use ash::vk;
use std::rc::Rc;

/// Fixed configuration; budgets are independent of hidden driver allocations.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Vulkan enumeration index. When absent, prefers a suitable integrated GPU,
    /// then discrete, virtual and finally CPU devices.
    pub device_index: Option<u32>,
    /// Bytes of explicit VkDeviceMemory allocations, including alignment and
    /// readback. Swapchain images belong to the window system and are not counted.
    pub memory_budget: u64,
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
    /// Estimated swapchain image bytes (width × height × 4 × image count), reported
    /// apart from device_bytes and the memory budget because WSI allocations are
    /// owned by the driver and opaque to Vulkan applications.
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
    /// Open layers and the images layers and blurs reuse.
    #[cfg(feature = "text")]
    pub(crate) layers: crate::layer::Layers,
    #[cfg(feature = "window")]
    pub(crate) swapchain: Option<crate::swapchain::Swapchain>,
    #[cfg(feature = "window")]
    pub(crate) window_size: [u32; 2],
    /// Regions of the next present that changed; empty means all of it.
    #[cfg(feature = "window")]
    pub(crate) damage: Vec<ash::vk::RectLayerKHR>,
    /// This window's surface; it outlives the swapchain and precedes the device.
    #[cfg(feature = "window")]
    pub(crate) surface: Option<crate::surface::Surface>,
    /// Host-visible clip and primitive storage, rewritten after each fence.
    buffers: [Option<Buffer>; 2],
    readback: Option<Buffer>,
    #[cfg(feature = "text")]
    pub(crate) text: crate::text::Text,
    pub(crate) pipeline: Pipeline,
    pub(crate) commands: Commands,
    pub(crate) device: Rc<Device>,
    pub(crate) recording: Recording,
    pub(crate) options: Options,
    name: String,
    pub(crate) busy: bool,
    image_ready: bool,
    /// Part of the current frame was already submitted; later passes load it.
    pub(crate) resumed: bool,
    /// The swapchain image this frame draws into, once its first submission
    /// acquired it: index, render-finished semaphore and suboptimal flag.
    #[cfg(feature = "window")]
    acquired: Option<(u32, vk::Semaphore, bool)>,
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
            #[cfg(feature = "text")]
            layers: Default::default(),
            #[cfg(feature = "window")]
            swapchain: None,
            #[cfg(feature = "window")]
            window_size: [0; 2],
            #[cfg(feature = "window")]
            damage: Vec::new(),
            #[cfg(feature = "window")]
            surface,
            buffers: [None, None],
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
            resumed: false,
            #[cfg(feature = "window")]
            acquired: None,
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
        self.resumed = false;
        // A failed or dropped frame that acquired an image already marked its
        // swapchain for teardown.
        #[cfg(feature = "window")]
        {
            self.acquired = None;
        }
        self.recording.clear();
        #[cfg(feature = "text")]
        {
            self.text.atlas.begin_frame();
            self.text.begin_textures();
            self.begin_layers();
        }
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
        #[cfg(feature = "text")]
        self.layers.clear();
        self.buffers = [None, None];
        self.readback = None;
        self.recording = Recording::default();
        #[cfg(feature = "text")]
        self.text.atlas.clear();
        self.image_ready = false;
        Ok(())
    }

    pub(crate) fn remaining(&self) -> u64 {
        self.options.memory_budget - self.stats().device_bytes
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

    pub(crate) fn base_bytes(&self) -> u64 {
        self.target.as_ref().map_or(0, Target::bytes)
            + self
                .buffers
                .iter()
                .flatten()
                .map(|b| b.allocation)
                .sum::<u64>()
            + self.readback.as_ref().map_or(0, |b| b.allocation)
            + self.layer_bytes()
    }

    fn layer_bytes(&self) -> u64 {
        #[cfg(feature = "text")]
        {
            self.layers.bytes()
        }
        #[cfg(not(feature = "text"))]
        {
            0
        }
    }

    /// The swapchain image this frame draws into, once acquired.
    #[cfg(all(feature = "window", feature = "text"))]
    pub(crate) fn acquired_index(&self) -> Option<u32> {
        self.acquired.map(|(index, _, _)| index)
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

    /// Uploads the records and begins a command buffer drawing them, after
    /// its atlas uploads.
    pub(crate) fn begin_submission(&mut self) -> Result {
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
        self.text
            .atlas
            .prepare_upload(&self.device, self.options.memory_budget - self.base_bytes())?;
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
        Ok(())
    }

    /// After a submission that is not the frame's last: waits for it and
    /// continues with empty records, unpinned atlas pages and free texture slots.
    pub(crate) fn continue_frame(&mut self) -> Result {
        self.wait()?;
        self.recording.primitives.clear();
        #[cfg(feature = "text")]
        {
            self.text.atlas.begin_frame();
            self.text.begin_textures();
        }
        Ok(())
    }

    /// Submits the recorded primitives. A frame that is not finished yet
    /// waits for this part, then continues with empty primitives, unpinned
    /// atlas pages and free texture slots; its last part is also presented.
    pub(crate) fn submit(&mut self, clear: Color, last: bool) -> Result {
        self.begin_submission()?;
        let target = self.target.as_ref().unwrap();
        let output = target.frames[1];
        #[cfg(feature = "window")]
        if self.acquired.is_none()
            && let Some(chain) = self.swapchain.as_mut()
        {
            self.acquired = Some(chain.acquire()?);
        }
        #[cfg(feature = "window")]
        let output = if let Some((index, _, _)) = self.acquired {
            self.swapchain.as_ref().unwrap().frames[index as usize]
        } else {
            output
        };
        let split = crate::commands::Split {
            resume: self.resumed,
            last,
        };
        self.commands.render(
            target,
            &self.pipeline,
            &self.recording,
            aegle_types::color_math::linear_rgba(clear.to_rgba()),
            output,
            split,
            #[cfg(feature = "text")]
            &self.text.pipeline,
        );
        let signals: &[vk::Semaphore] = &[];
        #[cfg(feature = "window")]
        let signal;
        #[cfg(feature = "window")]
        let signals = match self.acquired {
            Some((_, semaphore, _)) if last => {
                signal = [semaphore];
                &signal[..]
            }
            _ => signals,
        };
        self.commands.submit_signal(self.device.queue, signals)?;
        #[cfg(feature = "text")]
        self.text.atlas.commit();
        self.busy = true;
        if !last {
            self.resumed = true;
            return self.continue_frame();
        }
        self.image_ready = true;
        #[cfg(feature = "window")]
        if let Some((index, _, suboptimal)) = self.acquired.take() {
            let damage = &self.damage[..];
            let damage = (self.device.incremental_present && !damage.is_empty()).then_some(damage);
            let presented = self.swapchain.as_mut().unwrap().present(
                self.device.queue,
                index,
                suboptimal,
                damage,
            );
            self.damage.clear();
            presented?;
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

#[cfg(not(feature = "text"))]
impl Renderer {
    /// Draws the records into the frame's target and continues the frame.
    pub(crate) fn flush_current(&mut self, clear: Color) -> Result {
        self.submit(clear, false)
    }
}
