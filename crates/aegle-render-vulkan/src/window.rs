//! A native window owner kept alive beyond its Vulkan objects.
#![allow(unsafe_code)]
use crate::{
    Error, Frame, Options, Renderer, Result, Stats, device::Device, surface::Surface,
    swapchain::Swapchain,
};
use aegle_scene::Color;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::rc::Rc;

/// Vulkan FIFO presentation for a retained native Wayland or Win32 owner.
///
/// Shares the offscreen renderer's scene/text pipelines. Opaque windows blend
/// directly into an sRGB swapchain image when available; transparent windows (or
/// UNORM-only surfaces) use the RGBA16F image plus an encoding pass that writes the
/// swapchain image. No CPU framebuffer/readback or redundant RGBA8 offscreen image
/// exists. One graphics submission is in flight at a time.
/// The native event loop remains the caller's responsibility.
///
/// Windows on one connection can share a Vulkan instance and logical device with
/// [`WindowRenderer::with_device`]; each keeps its own swapchain, pipelines and
/// glyph atlas.
pub struct WindowRenderer<W> {
    renderer: Renderer,
    // Drops after Renderer, its Vulkan surface and all pending presentation.
    _window: W,
}
impl<W: HasDisplayHandle + HasWindowHandle> WindowRenderer<W> {
    /// Creates a Vulkan device and surface, retaining the supplied native owner.
    /// Images are allocated by the first nonzero `begin_frame`.
    ///
    /// # Safety
    /// `window` must keep both native handles valid and paired until this renderer
    /// is dropped; neither native window nor display may be destroyed externally.
    /// An owned platform lease retaining the window and connection satisfies this.
    /// Handle access and all renderer calls must obey the platform's thread rules.
    /// Raw-window-handle's short borrowed handles alone do not prove this lifetime.
    pub unsafe fn new(window: W, options: Options) -> Result<Self> {
        let display = window
            .display_handle()
            .map_err(|_| Error::Unsupported("native display handle unavailable"))?
            .as_raw();
        let handle = window
            .window_handle()
            .map_err(|_| Error::Unsupported("native window handle unavailable"))?
            .as_raw();
        let (device, surface) = Device::for_window(options.device_index, display, handle)?;
        Ok(Self {
            renderer: Renderer::with_device(options, Rc::new(device), Some(surface))?,
            _window: window,
        })
    }

    /// Creates a window renderer on a device another window already created,
    /// without a second Vulkan instance, device or queue. The device's queue must
    /// be able to present to this window; otherwise an unsupported error returns.
    ///
    /// # Safety
    /// Same contract as [`Self::new`]. Both windows must belong to the same
    /// display connection, and all renderers on the device run on one thread.
    pub unsafe fn with_device(window: W, options: Options, device: &SharedDevice) -> Result<Self> {
        let display = window
            .display_handle()
            .map_err(|_| Error::Unsupported("native display handle unavailable"))?
            .as_raw();
        let handle = window
            .window_handle()
            .map_err(|_| Error::Unsupported("native window handle unavailable"))?
            .as_raw();
        let shared = &device.0;
        let surface = Surface::new(shared.entry(), &shared.instance, display, handle)?;
        surface.supports(&shared.instance, shared.physical, shared.family)?;
        Ok(Self {
            renderer: Renderer::with_device(options, shared.clone(), Some(surface))?,
            _window: window,
        })
    }
}

/// A Vulkan instance and logical device shared by several [`WindowRenderer`]s on
/// one thread. The last renderer or handle to drop destroys it.
#[derive(Clone)]
pub struct SharedDevice(Rc<Device>);

#[cfg(feature = "text")]
impl SharedDevice {
    /// See [`Renderer::register_texture`].
    ///
    /// # Safety
    /// As for [`Renderer::register_texture`].
    pub unsafe fn register_texture(
        &self,
        view: ash::vk::ImageView,
        extent: [u32; 2],
    ) -> Result<crate::TextureId> {
        // SAFETY: forwarded to the caller.
        unsafe { self.0.textures.register(view, extent) }
    }
    /// See [`Renderer::unregister_texture`].
    pub fn unregister_texture(&self, id: crate::TextureId) -> bool {
        self.0.textures.unregister(id)
    }
    /// See [`Renderer::raw_device`].
    pub fn raw_device(&self) -> crate::RawDevice {
        self.0.raw_handles()
    }
}

impl<W> WindowRenderer<W> {
    /// A handle to this renderer's device for [`WindowRenderer::with_device`].
    pub fn shared_device(&self) -> SharedDevice {
        SharedDevice(self.renderer.device.clone())
    }
}
impl<W> WindowRenderer<W> {
    /// Starts a complete native frame. A zero or minimized surface returns None
    /// and releases its size-dependent images. Same-sized images are reused.
    /// `finish` presents; dropping a frame never acquires or presents an image.
    /// Opaque surfaces require an opaque clear color. `SurfaceOutOfDate` asks the caller to retain dirty state and schedule redraw.
    /// The caller must set platform scale/frame callbacks before finish, and use
    /// the actual [`Self::extent`] for layout if the surface constrains its size.
    pub fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        clear: Color,
    ) -> Result<Option<Frame<'_>>> {
        let Some([width, height]) =
            self.renderer
                .prepare_window(width, height)
                .map_err(|error| match error {
                    Error::Vulkan(ash::vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        Error::SurfaceOutOfDate
                    }
                    other => other,
                })?
        else {
            return Ok(None);
        };
        if !self.renderer.swapchain.as_ref().unwrap().transparent && clear.to_rgba()[3] != 255 {
            return Err(Error::Unsupported(
                "opaque native surface requires an opaque clear color",
            ));
        }
        self.renderer.begin_frame(width, height, clear).map(Some)
    }
    /// Whether the allocated swapchain preserves premultiplied window alpha.
    pub fn supports_transparency(&self) -> bool {
        self.renderer
            .swapchain
            .as_ref()
            .is_some_and(|chain| chain.transparent)
    }
    /// Current physical extent, or [0, 0] when suspended or not yet allocated.
    pub fn extent(&self) -> [u32; 2] {
        self.renderer
            .swapchain
            .as_ref()
            .map_or([0; 2], |chain| [chain.extent.width, chain.extent.height])
    }
    /// Name of the selected hardware device or Vulkan software driver.
    pub fn device_name(&self) -> &str {
        self.renderer.device_name()
    }
    /// Actual explicit allocations plus a separate opaque-swapchain byte estimate.
    pub fn stats(&self) -> Stats {
        self.renderer.stats()
    }
    /// Glyph cache/atlas storage, if optional text rendering is enabled.
    #[cfg(feature = "text")]
    pub fn text_stats(&self) -> crate::TextStats {
        self.renderer.text_stats()
    }
    /// Waits for graphics work. Presentation itself remains compositor-controlled.
    pub fn wait(&mut self) -> Result {
        self.renderer.wait()
    }
    /// Waits, releases swapchain/images/cache allocations, and keeps pipelines.
    pub fn release_images(&mut self) -> Result {
        self.renderer.release_images()
    }
}
impl Renderer {
    fn prepare_window(&mut self, width: u32, height: u32) -> Result<Option<[u32; 2]>> {
        self.wait()?;
        if width == 0 || height == 0 {
            self.release_swapchain()?;
            self.target = None;
            return Ok(None);
        }
        if self.window_size != [width, height]
            || self.swapchain.as_ref().is_none_or(|chain| chain.dirty)
        {
            self.release_swapchain()?;
            self.target = None;
            self.swapchain = Swapchain::new(
                &self.device,
                self.surface
                    .as_ref()
                    .expect("window renderer has a surface"),
                &self.pipeline,
                width,
                height,
                self.options.transparent,
            )?;
            self.window_size = [width, height];
        }
        Ok(self
            .swapchain
            .as_ref()
            .map(|chain| [chain.extent.width, chain.extent.height]))
    }
    pub(crate) fn release_swapchain(&mut self) -> Result {
        if self.swapchain.is_some() {
            // SAFETY: Exclusive mutable renderer access serializes this device.
            // Graphics fences do not cover present semaphore consumption; queue
            // idle is required before destroying the old image-specific semaphores.
            let result = unsafe { self.device.raw.device_wait_idle() };
            self.swapchain = None;
            result?;
        }
        Ok(())
    }
}
