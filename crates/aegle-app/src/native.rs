#[cfg(target_os = "windows")]
use crate::platform::Win32 as Platform;
use crate::platform::{PixelSize, WindowId};
#[cfg(target_os = "linux")]
use crate::platform::{Wayland as Platform, WlSeat};
#[cfg(feature = "software")]
use aegle_render_software::Renderer;
#[cfg(feature = "motion")]
use aegle_ui::Transition;
use aegle_ui::{Container, Modifiers, Result, Size, TextSystem, Theme, Ui, UiError};
#[cfg(feature = "motion")]
use std::time::Instant;
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::{Rc, Weak},
};

/// Explicit rendering policy. Backend failures are returned without switching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererBackend {
    /// CPU rendering into the platform's native software surface.
    /// Requires the `software` feature.
    Software,
    /// Vulkan geometry and glyph rendering directly into a native swapchain.
    /// Requires the `vulkan` feature and a Vulkan-capable driver.
    Vulkan,
    /// Portable wgpu geometry and glyph rendering directly into a native surface
    /// on Vulkan, Metal or Direct3D 12. Requires the `wgpu` feature.
    Wgpu,
}

impl Default for RendererBackend {
    fn default() -> Self {
        if cfg!(feature = "software") {
            Self::Software
        } else if cfg!(feature = "vulkan") {
            Self::Vulkan
        } else if cfg!(feature = "wgpu") {
            Self::Wgpu
        } else {
            Self::Software
        }
    }
}

/// Shared native application settings. Budgets exclude font metadata and trees.
#[derive(Clone, Debug)]
pub struct AppOptions {
    /// Desktop application identifier.
    pub app_id: String,
    /// Window theme while the system reports neither a dark nor a high-contrast
    /// preference that the fields below accept.
    pub theme: Theme,
    /// Theme while the system prefers a dark color scheme. Defaults to
    /// [`Theme::dark`]; `None` ignores the color-scheme preference.
    pub dark_theme: Option<Theme>,
    /// Theme while the system requests high contrast; it takes precedence over
    /// `dark_theme`. Defaults to [`Theme::high_contrast`]; `None` ignores it.
    pub high_contrast_theme: Option<Theme>,
    /// Explicit renderer selection; software is the default when compiled in.
    pub renderer: RendererBackend,
    /// Per-window Vulkan device, recording and glyph-cache budgets.
    #[cfg(feature = "vulkan")]
    pub vulkan: crate::VulkanOptions,
    /// Per-window wgpu glyph atlas size and transparency.
    #[cfg(feature = "wgpu")]
    pub wgpu: crate::WgpuOptions,
    /// Bytes of reusable software coverage and clipping masks, at most nine
    /// bytes per window pixel. Default: unlimited; set a limit on small devices.
    pub mask_budget: usize,
    /// Initial transition policy for each window's subsequently created interactive
    /// controls. Defaults to 120 ms ease-out; `None` disables this policy.
    #[cfg(feature = "motion")]
    pub transition: Option<Transition>,
    /// Explicit reduced-motion setting for windows. `None`, the default,
    /// follows the system preference and is false when none is reported.
    #[cfg(feature = "motion")]
    pub reduced_motion: Option<bool>,
    /// Text size as a percentage of the themes' (50–400) that scales font size
    /// and control height. `None`, the default, follows the system's text size.
    pub text_scale: Option<u16>,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            app_id: "org.aegle.app".into(),
            theme: Theme::default(),
            dark_theme: Some(Theme::dark()),
            high_contrast_theme: Some(Theme::high_contrast()),
            renderer: RendererBackend::default(),
            #[cfg(feature = "vulkan")]
            vulkan: crate::VulkanOptions::default(),
            #[cfg(feature = "wgpu")]
            wgpu: crate::WgpuOptions::default(),
            mask_budget: usize::MAX,
            #[cfg(feature = "motion")]
            transition: Some(Transition::default()),
            #[cfg(feature = "motion")]
            reduced_motion: None,
            text_scale: None,
        }
    }
}

/// Initial window size, presentation budget and Wayland surface role.
#[derive(Clone, Copy, Debug)]
pub struct WindowOptions {
    /// Preferred width in logical pixels; the compositor may override it.
    pub width: u32,
    /// Preferred height in logical pixels; the compositor may override it.
    pub height: u32,
    /// Maximum software presentation bytes for this window: two window-sized
    /// buffers at most. Default: unlimited; set a limit on small devices.
    /// GPU renderers use `AppOptions::vulkan` or `AppOptions::wgpu` instead.
    pub buffer_budget: usize,
    /// Wayland layer-shell placement instead of a toplevel, for panels,
    /// docks and overlays. Default: `None`.
    #[cfg(target_os = "linux")]
    pub layer: Option<crate::platform::LayerOptions>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 480,
            buffer_budget: usize::MAX,
            #[cfg(target_os = "linux")]
            layer: None,
        }
    }
}

/// Main-thread native application with shared fonts and explicit rendering.
///
/// Each window owns an independent retained tree. Callbacks run without a
/// native-runtime or tree borrow, and may modify or close any window. A single
/// active keyboard seat owns each window's logical focus domain. There is no
/// polling timer. Native failures terminate [`Self::run`] and are returned to
/// the caller; callback errors go to [`Self::on_error`].
pub struct App {
    pub(crate) runtime: Rc<RefCell<Runtime>>,
    pub(crate) dispatching: Cell<bool>,
    pub(crate) on_error: RefCell<Option<crate::native_loop::ErrorHandler>>,
}

pub(crate) struct Runtime {
    // Presenters and their window leases must be destroyed before the event loop.
    pub windows: Vec<Entry>,
    /// GPU devices shared by all windows, created with the first window and
    /// dropped before the platform that owns their surfaces' connection.
    #[cfg(feature = "vulkan")]
    pub shared_vulkan: Option<aegle_render_vulkan::SharedDevice>,
    #[cfg(feature = "wgpu")]
    pub shared_wgpu: Option<aegle_render_wgpu::SharedGpu>,
    pub backend: Platform,
    pub fonts: Rc<RefCell<TextSystem>>,
    #[cfg(feature = "software")]
    pub renderer: Option<Renderer>,
    pub options: AppOptions,
    /// UI-thread ends of [`UiProxy`] senders.
    pub proxies: Vec<Rc<RefCell<dyn crate::native_proxy::Drain>>>,
    /// Last system preferences applied to the windows.
    pub preferences: crate::platform::Preferences,
    #[cfg(feature = "motion")]
    pub clock: Instant,
}

pub(crate) struct Entry {
    #[cfg(feature = "vulkan")]
    pub gpu: Option<aegle_render_vulkan::WindowRenderer<crate::platform::WindowSurface>>,
    #[cfg(feature = "wgpu")]
    pub wgpu: Option<aegle_render_wgpu::WindowRenderer<crate::platform::WindowSurface>>,
    pub id: WindowId,
    pub ui: Rc<Ui>,
    /// Last cursor shape sent to the platform.
    pub cursor: aegle_ui::Cursor,
    #[cfg(any(
        all(feature = "unix-accessibility", target_os = "linux"),
        all(feature = "windows-accessibility", target_os = "windows")
    ))]
    pub title: String,
    #[cfg(target_os = "linux")]
    pub seat: Option<WlSeat>,
    /// Recent finger-scroll samples (compositor milliseconds, displacement).
    #[cfg(all(target_os = "linux", feature = "motion"))]
    pub flick: Vec<(u32, aegle_ui::Point)>,
    pub modifiers: Modifiers,
    /// Maps this window's platform input timestamps onto `Instant`.
    pub clock: crate::event_clock::EventClock,
    pub ready: bool,
    #[cfg(all(feature = "windows-accessibility", target_os = "windows"))]
    pub access_scale: f64,
    #[cfg(any(
        all(feature = "unix-accessibility", target_os = "linux"),
        all(feature = "windows-accessibility", target_os = "windows")
    ))]
    pub accessibility: crate::native_access::Adapter,
    #[cfg(any(
        all(feature = "unix-accessibility", target_os = "linux"),
        all(feature = "windows-accessibility", target_os = "windows")
    ))]
    pub initial_access: bool,
}

/// Weak ordinary-window handle; dereferences to its root [`Container`].
///
/// Dropping this handle keeps the window alive. [`Self::close`] removes the
/// native window and its retained tree, invalidating all of its control handles.
#[derive(Clone)]
pub struct Window {
    runtime: Weak<RefCell<Runtime>>,
    id: WindowId,
    root: Container,
}

impl App {
    /// The wgpu device shared by this app's windows, once a window using
    /// [`RendererBackend::Wgpu`] exists. Create textures on it and register
    /// them to draw them with `SceneBuilder::texture`.
    #[cfg(feature = "wgpu")]
    pub fn wgpu(&self) -> Option<aegle_render_wgpu::SharedGpu> {
        self.runtime.borrow().shared_wgpu.clone()
    }

    /// The Vulkan device shared by this app's windows, once a window using
    /// [`RendererBackend::Vulkan`] exists.
    #[cfg(feature = "vulkan")]
    pub fn vulkan(&self) -> Option<aegle_render_vulkan::SharedDevice> {
        self.runtime.borrow().shared_vulkan.clone()
    }

    /// Connects to the native platform and discovers installed system fonts.
    #[cfg(feature = "system-fonts")]
    pub fn new() -> Result<Self> {
        Self::with_options(AppOptions::default())
    }

    /// Builds a retained interface and runs it with the default application settings.
    /// A compiled markup builder can return typed weak handles; the application
    /// owns its windows independently of that return value.
    #[cfg(feature = "system-fonts")]
    pub fn run_ui<T>(build: impl FnOnce(&Self) -> Result<T>) -> Result<()> {
        let app = Self::new()?;
        build(&app)?;
        app.run()
    }

    /// Connects with explicit settings and system font discovery.
    #[cfg(feature = "system-fonts")]
    pub fn with_options(options: AppOptions) -> Result<Self> {
        Self::with_fonts(TextSystem::system(), options)
    }

    /// Connects using an explicit font collection, without system discovery.
    /// Register application fonts and configure generic fallback families first.
    pub fn with_fonts(fonts: TextSystem, options: AppOptions) -> Result<Self> {
        for theme in [
            Some(options.theme),
            options.dark_theme,
            options.high_contrast_theme,
        ] {
            theme.as_ref().map(Theme::validate).transpose()?;
        }
        if options.app_id.len() > 4000 || options.app_id.contains('\0') {
            return Err("application identifier exceeds 4000 bytes or contains NUL".into());
        }
        crate::native_render::validate_backend(options.renderer)?;
        let backend = Platform::connect()?;
        let runtime = Runtime {
            preferences: backend.preferences(),
            backend,
            windows: Vec::new(),
            #[cfg(feature = "vulkan")]
            shared_vulkan: None,
            #[cfg(feature = "wgpu")]
            shared_wgpu: None,
            fonts: Rc::new(RefCell::new(fonts)),
            #[cfg(feature = "software")]
            renderer: (options.renderer == RendererBackend::Software)
                .then(|| Renderer::new(options.mask_budget)),
            options,
            proxies: Vec::new(),
            #[cfg(feature = "motion")]
            clock: Instant::now(),
        };
        Ok(Self {
            runtime: Rc::new(RefCell::new(runtime)),
            dispatching: Cell::new(false),
            on_error: RefCell::new(None),
        })
    }

    /// System appearance preferences last applied to windows, for applications
    /// that resolve their own palettes or skins. `None` means unreported.
    pub fn preferences(&self) -> crate::Preferences {
        self.runtime.borrow().preferences
    }

    /// Creates a window with default size and a column root container.
    pub fn window(&self, title: &str) -> Result<Window> {
        self.window_with_options(title, WindowOptions::default())
    }

    /// Creates an independently owned tree on the application's shared connection.
    pub fn window_with_options(&self, title: &str, options: WindowOptions) -> Result<Window> {
        let mut runtime = self.runtime.borrow_mut();
        let ui = Rc::new(Ui::with_fonts(runtime.fonts.clone(), runtime.theme())?);
        // New windows join the application clock before callers can start a
        // transition, including windows created long after the first dispatch.
        #[cfg(feature = "motion")]
        {
            ui.advance_animations(runtime.clock.elapsed())?;
            ui.set_default_transition(runtime.options.transition)?;
            ui.set_reduced_motion(runtime.reduced_motion())?;
        }
        ui.set_double_click(runtime.double_click(), 4.0)?;
        ui.resize(Size::new(options.width as f32, options.height as f32))?;
        let app_id = runtime.options.app_id.clone();
        let id = runtime
            .backend
            .create_window(crate::platform::WindowOptions {
                title,
                app_id: &app_id,
                size: PixelSize {
                    width: options.width,
                    height: options.height,
                },
                buffer_budget: options.buffer_budget,
                #[cfg(target_os = "linux")]
                layer: options.layer,
            })?;
        #[cfg(feature = "vulkan")]
        let gpu = match crate::native_render::create_gpu(&mut runtime, id) {
            Ok(gpu) => gpu,
            Err(error) => {
                runtime.backend.remove_window(id)?;
                return Err(error);
            }
        };
        #[cfg(feature = "wgpu")]
        let wgpu = match crate::native_render::create_wgpu(&mut runtime, id) {
            Ok(wgpu) => wgpu,
            Err(error) => {
                runtime.backend.remove_window(id)?;
                return Err(error);
            }
        };
        #[cfg(any(
            all(feature = "unix-accessibility", target_os = "linux"),
            all(feature = "windows-accessibility", target_os = "windows")
        ))]
        let accessibility = match crate::native_access::create(&mut runtime, id) {
            Ok(adapter) => adapter,
            Err(error) => {
                #[cfg(feature = "vulkan")]
                drop(gpu);
                #[cfg(feature = "wgpu")]
                drop(wgpu);
                runtime.backend.remove_window(id)?;
                return Err(error);
            }
        };
        let root = ui.root();
        runtime.windows.push(Entry {
            #[cfg(feature = "vulkan")]
            gpu,
            #[cfg(feature = "wgpu")]
            wgpu,
            id,
            ui,
            cursor: aegle_ui::Cursor::Default,
            #[cfg(target_os = "linux")]
            seat: None,
            #[cfg(all(target_os = "linux", feature = "motion"))]
            flick: Vec::new(),
            modifiers: Modifiers::default(),
            clock: Default::default(),
            ready: false,
            #[cfg(all(feature = "windows-accessibility", target_os = "windows"))]
            access_scale: 1.0,
            #[cfg(any(
                all(feature = "unix-accessibility", target_os = "linux"),
                all(feature = "windows-accessibility", target_os = "windows")
            ))]
            title: title.into(),
            #[cfg(any(
                all(feature = "unix-accessibility", target_os = "linux"),
                all(feature = "windows-accessibility", target_os = "windows")
            ))]
            accessibility,
            #[cfg(any(
                all(feature = "unix-accessibility", target_os = "linux"),
                all(feature = "windows-accessibility", target_os = "windows")
            ))]
            initial_access: false,
        });
        Ok(Window {
            runtime: Rc::downgrade(&self.runtime),
            id,
            root,
        })
    }

    /// Whether this platform supports native composition (Wayland text-input-v3
    /// or the Windows IMM compatibility path). An unavailable requested IME is
    /// returned as a capability error from the event loop.
    pub fn ime_available(&self) -> bool {
        self.runtime.borrow().backend.ime_available()
    }

    /// Waits for native work until the final window closes or an error occurs.
    pub fn run(self) -> Result<()> {
        self.run_loop()
    }
}

impl Window {
    /// Closes this window immediately; subsequent control operations fail.
    pub fn close(&self) -> Result<()> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let mut runtime = runtime.borrow_mut();
        runtime.close(self.id)?;
        Ok(())
    }

    /// Applies a new theme to this window without replacing retained controls.
    /// System preference changes replace a window's theme only while it still
    /// equals the theme resolved before the change.
    pub fn set_theme(&self, theme: Theme) -> Result<()> {
        self.ui()?.set_theme(theme)
    }

    /// Sets this window's explicit reduced-motion preference. Enabling it snaps
    /// active transitions to their targets without changing focus or text.
    /// System changes apply only while the window keeps the previous resolved value.
    #[cfg(feature = "motion")]
    pub fn set_reduced_motion(&self, reduced: bool) -> Result<()> {
        self.ui()?.set_reduced_motion(reduced)
    }

    /// Installs this window's key handler, which sees every key before the
    /// focused control; see [`Ui::on_key`].
    pub fn on_key(
        &self,
        handler: impl FnMut(aegle_ui::KeyEvent<'_>) -> Result<bool> + 'static,
    ) -> Result<()> {
        self.ui()?.on_key(handler)
    }

    /// Removes this window's key handler.
    pub fn clear_on_key(&self) -> Result<()> {
        self.ui()?.clear_on_key()
    }

    fn ui(&self) -> Result<Rc<Ui>> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let ui = runtime
            .borrow()
            .windows
            .iter()
            .find(|entry| entry.id == self.id)
            .ok_or(UiError::DeadHandle)?
            .ui
            .clone();
        Ok(ui)
    }
}

impl Deref for Window {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.root
    }
}

impl Runtime {
    pub(crate) fn close(&mut self, id: WindowId) -> Result<()> {
        let index = self
            .windows
            .iter()
            .position(|entry| entry.id == id)
            .ok_or(UiError::DeadHandle)?;
        self.windows[index].ui.close()?;
        // Destroy the GPU surface and swapchain while the platform still owns
        // its window, then remove the window and queued native events.
        drop(self.windows.remove(index));
        self.backend.remove_window(id)?;
        Ok(())
    }
}
