use crate::Native;
use crate::renderer::{Renderer, RendererChoice};
use aegle_platform::{PixelSize, Platform, SeatId, WindowId};
#[cfg(feature = "motion")]
use aegle_ui::Transition;
use aegle_ui::{Container, Modifiers, OrFail, Result, Size, TextSystem, Theme, Ui, UiError};
#[cfg(feature = "motion")]
use std::time::Instant;
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::{Rc, Weak},
};

// Renderer selection lives in `crate::renderer`: every backend is one
// `Renderer` implementation, so the host has no backend enum to match on.

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
    /// Explicit renderer selection; the build's first backend by default.
    pub renderer: RendererChoice,
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
            renderer: RendererChoice::default(),
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

/// Initial window size, presentation budget and surface role.
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
    /// Desktop-shell layer placement instead of an ordinary window, for
    /// panels, docks and overlays (Wayland layer-shell). Window creation
    /// fails where the platform has no layers. Default: `None`.
    pub layer: Option<crate::LayerOptions>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 480,
            buffer_budget: usize::MAX,
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
    /// The desktop connection for controls' file dialogs, made on first use.
    #[cfg(feature = "desktop-services")]
    pub(crate) files: RefCell<Option<crate::native_files::Service>>,
}

pub(crate) struct Runtime {
    // Fields drop in declaration order: window presenters and their native
    // surface leases, then the renderer's devices, then the platform connection
    // and the event loop that owns it. Reordering these breaks that contract.
    pub windows: Vec<Entry>,
    /// The selected backend, or `None` in a build with no renderer at all.
    ///
    /// Holds each GPU window's surface and swapchain plus the shared device, so
    /// it must drop before `backend` releases the connection those handles
    /// belong to.
    pub renderer: Option<Box<dyn Renderer>>,
    pub backend: Native,
    pub fonts: Rc<RefCell<TextSystem>>,
    pub options: AppOptions,
    /// UI-thread ends of [`UiProxy`] senders.
    pub proxies: Vec<Rc<RefCell<dyn crate::native_proxy::Drain>>>,
    /// Last system preferences applied to the windows.
    pub preferences: crate::Preferences,
    #[cfg(feature = "motion")]
    pub clock: Instant,
}

pub(crate) struct Entry {
    pub id: WindowId,
    pub ui: Rc<Ui>,
    /// Last cursor shape sent to the platform.
    pub cursor: aegle_ui::Cursor,
    #[cfg(access)]
    pub title: String,
    /// The seat whose keyboard and pointer drive this window's focus domain.
    pub seat: Option<SeatId>,
    /// Recent scroll samples (platform milliseconds, displacement) for
    /// kinetic scrolling after the fingers lift.
    #[cfg(feature = "motion")]
    pub flick: Vec<(u32, aegle_ui::Point)>,
    pub modifiers: Modifiers,
    /// Maps this window's platform input timestamps onto `Instant`.
    pub clock: crate::event_clock::EventClock,
    pub ready: bool,
    #[cfg(access)]
    pub accessibility: crate::native_access::Access,
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
    /// The wgpu device shared by this app's windows, once a window exists.
    ///
    /// Create textures on it and register them to draw them with
    /// `SceneBuilder::texture`. `None` unless the wgpu backend is in use.
    #[cfg(feature = "wgpu")]
    pub fn wgpu(&self) -> Option<aegle_render_wgpu::SharedGpu> {
        self.runtime.borrow().renderer.as_ref()?.wgpu_device()
    }

    /// The Vulkan device shared by this app's windows, once a window exists.
    ///
    /// `None` unless the Vulkan backend is in use.
    #[cfg(feature = "vulkan")]
    pub fn vulkan(&self) -> Option<aegle_render_vulkan::SharedDevice> {
        self.runtime.borrow().renderer.as_ref()?.vulkan_device()
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
        // Resolving the choice before connecting keeps a misconfigured build
        // from touching the display server at all.
        let renderer = Some(crate::renderer::create(
            &options,
            options.renderer.resolve()?,
        )?);
        let backend = Native::connect()?;
        let runtime = Runtime {
            preferences: backend.preferences(),
            backend,
            windows: Vec::new(),
            fonts: Rc::new(RefCell::new(fonts)),
            renderer,
            options,
            proxies: Vec::new(),
            #[cfg(feature = "motion")]
            clock: Instant::now(),
        };
        Ok(Self {
            runtime: Rc::new(RefCell::new(runtime)),
            dispatching: Cell::new(false),
            on_error: RefCell::new(None),
            #[cfg(feature = "desktop-services")]
            files: RefCell::new(None),
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
            ui.set_default_transition(runtime.options.transition);
            ui.set_reduced_motion(runtime.reduced_motion());
        }
        ui.set_double_click(runtime.double_click(), 4.0);
        ui.resize(Size::new(options.width as f32, options.height as f32));
        let app_id = runtime.options.app_id.clone();
        let id = runtime
            .backend
            .create_window(aegle_platform::WindowOptions {
                title,
                app_id: &app_id,
                size: PixelSize {
                    width: options.width,
                    height: options.height,
                },
                buffer_budget: options.buffer_budget,
                layer: options.layer,
            })?;
        // The renderer and the platform are separate fields, so they are
        // borrowed separately while the window is handed over. A renderer that
        // cannot take this window must not leave the native window behind, and
        // whatever it already acquired must go first.
        let Runtime {
            renderer: slot,
            backend,
            options: settings,
            ..
        } = &mut *runtime;
        if let Some(renderer) = slot.as_mut() {
            let outcome = renderer.add_window(
                id,
                &mut crate::renderer::Setup {
                    backend,
                    options: settings,
                },
            );
            if let Err(error) = outcome {
                renderer.remove_window(id);
                runtime.backend.remove_window(id)?;
                return Err(error);
            }
        }
        #[cfg(access)]
        let accessibility = match crate::native_access::Access::new(&mut runtime.backend, id) {
            Ok(adapter) => adapter,
            Err(error) => {
                if let Some(renderer) = runtime.renderer.as_mut() {
                    renderer.remove_window(id);
                }
                runtime.backend.remove_window(id)?;
                return Err(error);
            }
        };
        let root = ui.root();
        runtime.windows.push(Entry {
            id,
            ui,
            cursor: aegle_ui::Cursor::Default,
            seat: None,
            #[cfg(feature = "motion")]
            flick: Vec::new(),
            modifiers: Modifiers::default(),
            clock: Default::default(),
            ready: false,
            #[cfg(access)]
            title: title.into(),
            #[cfg(access)]
            accessibility,
        });
        Ok(Window {
            runtime: Rc::downgrade(&self.runtime),
            id,
            root,
        })
    }

    /// Whether this platform supports native composition (Wayland text-input-v3
    /// or the Windows Text Services Framework). An unavailable requested IME is
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
    /// Closes this window immediately and removes its controls; their
    /// handles then panic like those of removed controls. Fails if the
    /// window was already closed or the platform fails to destroy it.
    pub fn close(&self) -> Result<()> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let mut runtime = runtime.borrow_mut();
        runtime.close(self.id)?;
        Ok(())
    }

    /// Retitles the window, also for assistive technology: at most 4000
    /// bytes without NUL. Layer surfaces have no title and fail.
    pub fn set_title(&self, title: &str) -> Result<()> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let mut runtime = runtime.borrow_mut();
        runtime.backend.set_title(self.id, title)?;
        #[cfg(access)]
        {
            let entry = runtime.windows.iter_mut().find(|entry| entry.id == self.id);
            entry.ok_or(UiError::DeadHandle)?.title = title.into();
            self.root
                .change(|state, id| state.invalidate(id, aegle_ui::Dirty::SEMANTICS));
        }
        Ok(())
    }

    /// This window's engine, for what concerns the whole window rather than
    /// one control: key handlers, reduced motion, tokens and default
    /// transitions. Controls keep using their handles; the window's root is
    /// this handle's [`Container`]. Panics once the window is closed.
    pub fn ui(&self) -> Rc<Ui> {
        let runtime = self.runtime.upgrade();
        let runtime = runtime.ok_or(UiError::DeadHandle).or_fail();
        let runtime = runtime.borrow();
        let entry = runtime.windows.iter().find(|entry| entry.id == self.id);
        entry.ok_or(UiError::DeadHandle).or_fail().ui.clone()
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
        self.windows[index].ui.close();
        // Destroy the renderer's state (GPU surfaces, swapchains) while the
        // platform still owns the window, then remove the window and its events.
        drop(self.windows.remove(index));
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.remove_window(id);
        }
        self.backend.remove_window(id)?;
        Ok(())
    }
}
