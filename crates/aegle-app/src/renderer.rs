//! Renderers as one uniform concept: every backend is a [`Renderer`].
//!
//! The host does not classify backends. A backend knows its name, keeps its own
//! per-window state, and presents one redraw; the host supplies the platform
//! connection, the retained tree and the theme background, and reports errors.
//! Rasterizing on the CPU is not a special case: tiny-skia, cairo, Vulkan and
//! wgpu are four implementations of the same trait, differing only in what they
//! do internally.
//!
//! Selection is by name ([`RendererChoice::named`]) or by the build's default
//! ([`RendererChoice::default`]), the first backend this build contains in the
//! compile-time order of [`built`]. No backend is privileged.

use aegle_platform::WindowId;
use aegle_ui::Result;

use aegle_platform::Platform;

use crate::native::{AppOptions, Runtime};

/// Everything a renderer needs to present one window's redraw.
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

/// A requested backend, resolved against this build.
///
/// A choice is a name resolved when the application starts; there is no backend
/// enum and no privileged member. [`Default`] means "this build's first
/// backend", resolved at [`App`](crate::App) creation. A name this build does
/// not contain is an error rather than a silent substitution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererChoice {
    /// `None` until resolved: take this build's default order.
    name: Option<&'static str>,
}

impl RendererChoice {
    /// The backend with this name, if this build contains it.
    ///
    /// Fails for a backend the build can name but does not contain, so callers
    /// never get a different rasterizer than the one they asked for.
    pub fn named(name: &'static str) -> Result<Self> {
        match built().contains(&name) {
            true => Ok(Self { name: Some(name) }),
            false => Err(format!(
                "this build has no {name} renderer; rebuild with --features {name}"
            )
            .into()),
        }
    }

    /// This backend's name, or the build's default, resolved once.
    ///
    /// Called at [`App`](crate::App) creation; an explicit name is always kept,
    /// so a build with several backends never overrides the request. Fails only
    /// when the build contains no renderer at all.
    pub fn resolve(&self) -> Result<&'static str> {
        match self.name {
            Some(name) => Ok(name),
            None => built().into_iter().next().ok_or_else(|| {
                "this build has no renderer; enable one of tiny-skia, cairo, vulkan or wgpu".into()
            }),
        }
    }

    /// This backend's name, or `None` while it still means "build default".
    pub fn name(&self) -> Option<&'static str> {
        self.name
    }
}

impl std::fmt::Display for RendererChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.name {
            Some(name) => formatter.write_str(name),
            None => formatter.write_str("default"),
        }
    }
}

/// Every backend name this crate knows, whether or not this build contains it.
///
/// A caller parsing a name from a command line uses this to tell a typo from a
/// backend that simply was not compiled in.
pub const KNOWN: &[&str] = &["tiny-skia", "cairo", "vulkan", "wgpu"];

/// The backends this build contains, in default preference order.
///
/// The order is the order of these blocks, so it is stable for a given feature
/// combination; the first entry is the default.
pub fn built() -> Vec<&'static str> {
    vec![
        #[cfg(feature = "tiny-skia")]
        Cpu::NAME,
        #[cfg(feature = "cairo")]
        Cairo::NAME,
        #[cfg(feature = "vulkan")]
        Vulkan::NAME,
        #[cfg(feature = "wgpu")]
        Wgpu::NAME,
    ]
}

/// Creates the backend named by `name` for a new application.
///
/// Renderers construct without a platform connection: the ones that need native
/// handles take them per window in [`Renderer::add_window`].
pub(crate) fn create(options: &AppOptions, name: &'static str) -> Result<Box<dyn Renderer>> {
    #[cfg(feature = "tiny-skia")]
    if name == Cpu::NAME {
        return Ok(Box::new(Cpu {
            renderer: aegle_render_tiny_skia::Renderer::new(options.mask_budget),
        }));
    }
    #[cfg(feature = "cairo")]
    if name == Cairo::NAME {
        return Ok(Box::new(Cairo {
            renderer: aegle_render_cairo::renderer(options.mask_budget)?,
        }));
    }
    #[cfg(feature = "vulkan")]
    if name == Vulkan::NAME {
        if cfg!(target_os = "macos") {
            return Err("Vulkan rendering is unavailable on macOS; use wgpu".into());
        }
        return Ok(Box::new(Vulkan::default()));
    }
    #[cfg(feature = "wgpu")]
    if name == Wgpu::NAME {
        return Ok(Box::new(Wgpu::default()));
    }
    let _ = (options, name);
    Err(format!("this build has no {name} renderer").into())
}

// The four backends. Each is a thin shell over `native_render`'s shared drawing
// code; what differs lives there, not in how the host treats them.

/// The shared CPU compositor over tiny-skia coverage.
#[cfg(feature = "tiny-skia")]
struct Cpu {
    renderer: aegle_render_tiny_skia::Renderer,
}

#[cfg(feature = "tiny-skia")]
impl Cpu {
    const NAME: &'static str = "tiny-skia";
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
impl Cairo {
    const NAME: &'static str = "cairo";
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
impl Vulkan {
    const NAME: &'static str = "vulkan";
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
impl Wgpu {
    const NAME: &'static str = "wgpu";
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
            renderer: slot,
            backend,
            windows,
            ..
        } = self;
        let Some(renderer) = slot.as_mut() else {
            return Ok(());
        };
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
