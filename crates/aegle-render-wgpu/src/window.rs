//! Presentation to a native surface owned by the caller.
#![allow(unsafe_code)]
use aegle_scene::Color;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use wgpu::{
    CompositeAlphaMode, CurrentSurfaceTexture, PresentMode, SurfaceConfiguration,
    SurfaceTargetUnsafe, TextureFormat, TextureUsages,
};

use crate::{Error, Frame, Options, Renderer, Result, frame::Presentation, gpu::Gpu};

/// FIFO presentation for a native window, sharing the offscreen renderer's
/// pipelines. Output goes straight into the swapchain image; no CPU framebuffer
/// or readback exists. The caller's event loop drives every frame.
pub struct WindowRenderer<W> {
    // Declaration order drops the device, then the surface, then the window.
    renderer: Renderer,
    surface: wgpu::Surface<'static>,
    format: TextureFormat,
    alpha: CompositeAlphaMode,
    configured: [u32; 2],
    _window: W,
}

impl<W: HasDisplayHandle + HasWindowHandle> WindowRenderer<W> {
    /// Creates a device compatible with `window`'s surface, retaining the owner.
    /// The swapchain is configured by the first nonzero `begin_frame`.
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
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        // SAFETY: the caller guarantees both handles outlive `self`, which owns
        // the surface and drops it before `_window`.
        let surface = unsafe {
            instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(display),
                raw_window_handle: handle,
            })
        }
        .map_err(Error::Surface)?;
        let (gpu, capabilities) = Gpu::connect(&instance, Some(&surface))?;
        let capabilities = capabilities.unwrap();
        // The resolve pass writes encoded bytes, so an sRGB format would encode twice.
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .ok_or(Error::Unsupported("non-sRGB surface format"))?;
        let has = |mode| capabilities.alpha_modes.contains(&mode);
        let alpha = if options.transparent && has(CompositeAlphaMode::PreMultiplied) {
            CompositeAlphaMode::PreMultiplied
        } else if has(CompositeAlphaMode::Opaque) {
            CompositeAlphaMode::Opaque
        } else {
            capabilities.alpha_modes[0]
        };
        Ok(Self {
            renderer: Renderer::with_gpu(gpu, options),
            surface,
            format,
            alpha,
            configured: [0; 2],
            _window: window,
        })
    }
}

impl<W> WindowRenderer<W> {
    /// Starts a complete native frame. A zero extent returns `None`, as does an
    /// occluded or timed-out surface. `finish` presents; dropping a frame never
    /// does. [`Error::SurfaceOutOfDate`] asks the caller to keep its dirty state
    /// and redraw. Opaque surfaces require an opaque clear color.
    pub fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        clear: Color,
    ) -> Result<Option<Frame<'_>>> {
        if width == 0 || height == 0 {
            self.configured = [0; 2];
            return Ok(None);
        }
        if !self.supports_transparency() && clear.to_rgba()[3] != 255 {
            return Err(Error::Unsupported(
                "opaque native surface requires an opaque clear color",
            ));
        }
        if self.configured != [width, height] {
            self.surface.configure(
                &self.renderer.gpu.device,
                &SurfaceConfiguration {
                    usage: TextureUsages::RENDER_ATTACHMENT,
                    format: self.format,
                    color_space: Default::default(),
                    width,
                    height,
                    present_mode: PresentMode::Fifo,
                    desired_maximum_frame_latency: 2,
                    alpha_mode: self.alpha,
                    view_formats: vec![],
                },
            );
            self.configured = [width, height];
        }
        let texture = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture) => texture,
            CurrentSurfaceTexture::Suboptimal(texture) => {
                self.configured = [0; 2];
                texture
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(None),
            CurrentSurfaceTexture::Outdated => {
                self.configured = [0; 2];
                return Err(Error::SurfaceOutOfDate);
            }
            CurrentSurfaceTexture::Lost => return Err(Error::SurfaceLost),
            CurrentSurfaceTexture::Validation => {
                return Err(Error::Unsupported("surface acquisition failed validation"));
            }
        };
        self.renderer.start(width, height, clear, false)?;
        Ok(Some(Frame::new(
            &mut self.renderer,
            Presentation::Surface(texture),
        )))
    }

    /// Whether the swapchain preserves premultiplied window alpha.
    pub fn supports_transparency(&self) -> bool {
        self.alpha == CompositeAlphaMode::PreMultiplied
    }

    /// Current physical extent, or `[0, 0]` while suspended or not yet configured.
    pub fn extent(&self) -> [u32; 2] {
        self.configured
    }

    /// Name of the selected adapter.
    pub fn device_name(&self) -> &str {
        self.renderer.device_name()
    }
}
