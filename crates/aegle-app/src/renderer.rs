//! Renderers as one uniform concept: every backend is a [`Renderer`].
//!
//! The host does not classify backends. A backend knows its own per-window
//! state and presents one redraw; the host supplies the platform connection, the
//! retained tree and the theme background, and reports errors. Rasterizing on
//! the CPU is not a special case: tiny-skia, cairo, Vulkan and wgpu are four
//! implementations of the same trait, differing only in what they do internally.
//!
//! A build contains exactly one of them. The `tiny-skia`, `cairo`, `vulkan` and
//! `wgpu` features are mutually exclusive, so the binary carries no backend it
//! cannot use and an application has nothing to choose at run time;
//! [`RENDERER`] names the one this build contains.

use aegle_platform::WindowId;
use aegle_ui::Result;

use aegle_platform::Platform;

use crate::native::{AppOptions, Runtime};

/// Everything a renderer needs to present one window's redraw.
#[cfg_attr(not(renderer), allow(dead_code))]
pub(crate) struct Present<'a> {
    /// The platform connection that owns the window.
    pub backend: &'a mut crate::Native,
    /// The retained tree of the window being presented.
    pub ui: &'a aegle_ui::Ui,
    /// Physical size, scale and redraw state reported by the platform.
    pub info: aegle_platform::WindowInfo,
    /// Root theme background.
    pub background: aegle_ui::Color,
}

/// What a renderer needs when a platform window is created for it.
pub(crate) struct Setup<'a> {
    /// The platform connection owning the new window.
    #[cfg_attr(not(any(feature = "vulkan", feature = "wgpu")), allow(dead_code))]
    pub backend: &'a mut crate::Native,
    /// Application settings, including this backend's options.
    #[cfg_attr(not(any(feature = "vulkan", feature = "wgpu")), allow(dead_code))]
    pub options: &'a AppOptions,
}

impl Setup<'_> {
    /// The Vulkan options of this application.
    #[cfg(feature = "vulkan")]
    pub(crate) fn vulkan(&self) -> crate::VulkanOptions {
        self.options.vulkan
    }

    /// The wgpu options of this application.
    #[cfg(feature = "wgpu")]
    pub(crate) fn wgpu(&self) -> crate::WgpuOptions {
        self.options.wgpu
    }
}

/// One rendering backend.
///
/// Implemented once per backend; the host holds one behind `Box<dyn Renderer>`
/// and never matches on a backend name.
pub(crate) trait Renderer {
    /// Creates this window's rendering state; the platform window already exists.
    fn add_window(&mut self, window: WindowId, setup: &mut Setup<'_>) -> Result<()>;

    /// Releases it while the platform still owns the window.
    fn remove_window(&mut self, window: WindowId);

    /// Draws and presents one redraw of `window`.
    ///
    /// Returns whether a frame reached the compositor.
    fn present(&mut self, window: WindowId, present: &mut Present<'_>) -> Result<bool>;

    /// The Vulkan device this app's windows share, once one exists.
    #[cfg(feature = "vulkan")]
    fn vulkan_device(&self) -> Option<aegle_render_vulkan::SharedDevice> {
        None
    }

    /// The wgpu device this app's windows share, once one exists.
    #[cfg(feature = "wgpu")]
    fn wgpu_device(&self) -> Option<aegle_render_wgpu::SharedGpu> {
        None
    }
}

/// The renderer this build contains, or `None` without a renderer feature.
///
/// The renderer features are mutually exclusive, so this is the backend every
/// [`App`](crate::App) of the build draws with. A build without one compiles,
/// but fails at `App` creation.
pub const RENDERER: Option<&str> = if cfg!(feature = "tiny-skia") {
    Some("tiny-skia")
} else if cfg!(feature = "cairo") {
    Some("cairo")
} else if cfg!(feature = "vulkan") {
    Some("vulkan")
} else if cfg!(feature = "wgpu") {
    Some("wgpu")
} else {
    None
};

/// Creates this build's backend for a new application.
///
/// Renderers construct without a platform connection: the ones that need native
/// handles take them per window in [`Renderer::add_window`]. A build without a
/// renderer fails here, before the application connects.
pub(crate) fn create(options: &AppOptions) -> Result<Box<dyn Renderer>> {
    // The GPU backends read their options per window, in `add_window`.
    let _ = options;
    #[cfg(feature = "tiny-skia")]
    return Ok(Box::new(Cpu {
        renderer: aegle_render_tiny_skia::Renderer::new(options.mask_budget),
    }));
    #[cfg(feature = "cairo")]
    return Ok(Box::new(Cairo {
        renderer: aegle_render_cairo::renderer(options.mask_budget)?,
    }));
    #[cfg(feature = "vulkan")]
    return match cfg!(target_os = "macos") {
        true => Err("Vulkan rendering is unavailable on macOS; build with wgpu instead".into()),
        false => Ok(Box::new(Vulkan::default())),
    };
    #[cfg(feature = "wgpu")]
    return Ok(Box::new(Wgpu::default()));
    #[cfg(not(renderer))]
    return Err(
        "this build has no renderer; enable one of the tiny-skia, cairo, vulkan or wgpu features"
            .into(),
    );
}

// The four backends, one of which a build compiles. Each is a thin shell over
// `native_render`'s shared drawing code; what differs lives there, not in how
// the host treats them.

/// The shared CPU compositor over tiny-skia coverage.
#[cfg(feature = "tiny-skia")]
struct Cpu {
    renderer: aegle_render_tiny_skia::Renderer,
}

#[cfg(feature = "tiny-skia")]
impl Renderer for Cpu {
    fn add_window(&mut self, _window: WindowId, _setup: &mut Setup<'_>) -> Result<()> {
        // One compositor serves every window: it shares the mask scratch and the
        // glyph cache, and no window needs its own resources.
        Ok(())
    }

    fn remove_window(&mut self, _window: WindowId) {}

    fn present(&mut self, window: WindowId, present: &mut Present<'_>) -> Result<bool> {
        crate::native_render::present_cpu(&mut self.renderer, window, present)
    }
}

/// The shared CPU compositor over system cairo coverage.
#[cfg(feature = "cairo")]
struct Cairo {
    renderer: aegle_render_cairo::Renderer,
}

#[cfg(feature = "cairo")]
impl Renderer for Cairo {
    fn add_window(&mut self, _window: WindowId, _setup: &mut Setup<'_>) -> Result<()> {
        Ok(())
    }

    fn remove_window(&mut self, _window: WindowId) {}

    fn present(&mut self, window: WindowId, present: &mut Present<'_>) -> Result<bool> {
        crate::native_render::present_cpu(&mut self.renderer, window, present)
    }
}

/// Direct Vulkan presentation through ash.
#[cfg(feature = "vulkan")]
#[derive(Default)]
struct Vulkan {
    windows: Vec<(
        WindowId,
        aegle_render_vulkan::WindowRenderer<crate::native_render::Surface>,
    )>,
    /// Device shared by every window, created with the first one.
    shared: Option<aegle_render_vulkan::SharedDevice>,
}

#[cfg(feature = "vulkan")]
impl Renderer for Vulkan {
    fn add_window(&mut self, window: WindowId, setup: &mut Setup<'_>) -> Result<()> {
        let renderer = crate::native_render::create_vulkan(setup, window, &self.shared)?;
        self.shared.get_or_insert_with(|| renderer.shared_device());
        self.windows.push((window, renderer));
        Ok(())
    }

    fn remove_window(&mut self, window: WindowId) {
        // The lease on the window's native handles must go before the platform
        // destroys the window.
        self.windows.retain(|(id, _)| *id != window);
    }

    fn present(&mut self, window: WindowId, present: &mut Present<'_>) -> Result<bool> {
        let renderer = self
            .windows
            .iter_mut()
            .find(|(id, _)| *id == window)
            .map(|(_, renderer)| renderer)
            .ok_or("presenting a window without a Vulkan renderer")?;
        crate::native_render::present_vulkan(renderer, window, present)
    }

    fn vulkan_device(&self) -> Option<aegle_render_vulkan::SharedDevice> {
        self.shared.clone()
    }
}

/// Portable wgpu presentation on Vulkan, Metal or Direct3D 12.
#[cfg(feature = "wgpu")]
#[derive(Default)]
struct Wgpu {
    windows: Vec<(
        WindowId,
        aegle_render_wgpu::WindowRenderer<crate::native_render::Surface>,
    )>,
    /// Device shared by every window, created with the first one.
    shared: Option<aegle_render_wgpu::SharedGpu>,
}

#[cfg(feature = "wgpu")]
impl Renderer for Wgpu {
    fn add_window(&mut self, window: WindowId, setup: &mut Setup<'_>) -> Result<()> {
        let renderer = crate::native_render::create_wgpu(setup, window, &self.shared)?;
        self.shared.get_or_insert_with(|| renderer.shared_gpu());
        self.windows.push((window, renderer));
        Ok(())
    }

    fn remove_window(&mut self, window: WindowId) {
        self.windows.retain(|(id, _)| *id != window);
    }

    fn present(&mut self, window: WindowId, present: &mut Present<'_>) -> Result<bool> {
        let renderer = self
            .windows
            .iter_mut()
            .find(|(id, _)| *id == window)
            .map(|(_, renderer)| renderer)
            .ok_or("presenting a window without a wgpu renderer")?;
        crate::native_render::present_wgpu(renderer, window, present)
    }

    fn wgpu_device(&self) -> Option<aegle_render_wgpu::SharedGpu> {
        self.shared.clone()
    }
}

impl Runtime {
    /// Presents every window that has pending work, then requests the next
    /// frame for windows that still want one.
    ///
    /// The host does no backend-specific work here: each redraw is one
    /// [`Renderer::present`] call, and anything a backend cannot draw surfaces
    /// as an error from that call.
    pub(crate) fn present(&mut self) -> Result<()> {
        let Runtime {
            renderer,
            backend,
            windows,
            ..
        } = self;
        for entry in windows.iter_mut() {
            if !std::mem::take(&mut entry.ready) {
                continue;
            }
            let id = entry.id;
            let info = backend.window_info(id)?;
            let background = entry.ui.background();
            // Each backend decides how much of the buffer it redraws; the host
            // only reports the window state and applies the result.
            let presented = {
                let mut present = Present {
                    backend,
                    ui: &entry.ui,
                    info,
                    background,
                };
                renderer.present(id, &mut present)
            }?;
            if presented {
                entry.ui.clear_damage();
            } else {
                backend.request_redraw(id)?;
            }
            #[cfg(feature = "motion")]
            let animating = entry.ui.has_animations();
            #[cfg(not(feature = "motion"))]
            let animating = false;
            if animating || entry.ui.wants_frames() {
                backend.request_redraw(id)?;
            }
        }
        Ok(())
    }
}
