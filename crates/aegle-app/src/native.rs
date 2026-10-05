#[cfg(feature = "motion")]
use crate::Transition;
use crate::{Container, Modifiers, Result, Size, TextSystem, Theme, Ui, UiError};
use aegle_platform_wayland::{PixelSize, Wayland, WindowId, WlSeat};
use aegle_render_software::Renderer;
#[cfg(feature = "motion")]
use std::time::Instant;
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::{Rc, Weak},
};

/// Shared native application settings. Budgets exclude font metadata and trees.
#[derive(Clone, Debug)]
pub struct AppOptions {
    /// Desktop application identifier.
    pub app_id: String,
    /// Initial theme for each new window.
    pub theme: Theme,
    /// Reusable software coverage and clipping storage. Default: 2 MiB.
    pub mask_budget: usize,
    /// Initial transition policy for each window's subsequently created buttons
    /// and editors. Defaults to 120 ms ease-out; `None` disables this policy.
    #[cfg(feature = "motion")]
    pub transition: Option<Transition>,
    /// Explicit reduced-motion preference for new windows. Defaults to false;
    /// operating-system preference discovery is not performed here.
    #[cfg(feature = "motion")]
    pub reduced_motion: bool,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            app_id: "org.aegle.app".into(),
            theme: Theme::default(),
            mask_budget: 2 * 1024 * 1024,
            #[cfg(feature = "motion")]
            transition: Some(Transition::default()),
            #[cfg(feature = "motion")]
            reduced_motion: false,
        }
    }
}

/// Initial ordinary window size and presentation budget.
#[derive(Clone, Copy, Debug)]
pub struct WindowOptions {
    /// Preferred width in logical pixels; the compositor may override it.
    pub width: u32,
    /// Preferred height in logical pixels; the compositor may override it.
    pub height: u32,
    /// Maximum live SHM mapping bytes for this window. Default: 16 MiB.
    pub buffer_budget: usize,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 480,
            buffer_budget: 16 * 1024 * 1024,
        }
    }
}

/// Main-thread Wayland application with shared fonts and software glyph cache.
///
/// Each window owns an independent retained tree. Callbacks run without a
/// native-runtime or tree borrow, and may modify or close any window. A single
/// active keyboard seat owns each window's logical focus domain. There is no
/// polling timer. Native failures and callback errors terminate [`Self::run`]
/// and are returned to the caller.
pub struct App {
    pub(crate) runtime: Rc<RefCell<Runtime>>,
    pub(crate) dispatching: Cell<bool>,
}

pub(crate) struct Runtime {
    pub backend: Wayland,
    pub windows: Vec<Entry>,
    pub fonts: Rc<RefCell<TextSystem>>,
    pub renderer: Renderer,
    pub options: AppOptions,
    #[cfg(feature = "motion")]
    pub clock: Instant,
}

pub(crate) struct Entry {
    pub id: WindowId,
    pub ui: Rc<Ui>,
    #[cfg(feature = "unix-accessibility")]
    pub title: String,
    pub seat: Option<WlSeat>,
    pub modifiers: Modifiers,
    pub ready: bool,
    #[cfg(feature = "unix-accessibility")]
    pub accessibility: aegle_access::UnixAdapter,
    #[cfg(feature = "unix-accessibility")]
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
    /// Connects to Wayland and discovers installed system fonts.
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
        options.theme.validate()?;
        if options.app_id.len() > 4000 || options.app_id.contains('\0') {
            return Err("application identifier exceeds 4000 bytes or contains NUL".into());
        }
        let runtime = Runtime {
            backend: Wayland::connect()?,
            windows: Vec::new(),
            fonts: Rc::new(RefCell::new(fonts)),
            renderer: Renderer::new(options.mask_budget),
            options,
            #[cfg(feature = "motion")]
            clock: Instant::now(),
        };
        Ok(Self {
            runtime: Rc::new(RefCell::new(runtime)),
            dispatching: Cell::new(false),
        })
    }

    /// Creates a window with default size and a column root container.
    pub fn window(&self, title: &str) -> Result<Window> {
        self.window_with_options(title, WindowOptions::default())
    }

    /// Creates an independently owned tree on the application's shared connection.
    pub fn window_with_options(&self, title: &str, options: WindowOptions) -> Result<Window> {
        let mut runtime = self.runtime.borrow_mut();
        let ui = Rc::new(Ui::with_fonts(
            runtime.fonts.clone(),
            runtime.options.theme,
        )?);
        // New windows join the application clock before callers can start a
        // transition, including windows created long after the first dispatch.
        #[cfg(feature = "motion")]
        {
            ui.advance_animations(runtime.clock.elapsed())?;
            ui.set_default_transition(runtime.options.transition)?;
            ui.set_reduced_motion(runtime.options.reduced_motion)?;
        }
        ui.resize(Size::new(options.width as f32, options.height as f32))?;
        #[cfg(feature = "unix-accessibility")]
        let wake = runtime.backend.wake_handle()?;
        let app_id = runtime.options.app_id.clone();
        let id = runtime
            .backend
            .create_window(aegle_platform_wayland::WindowOptions {
                title,
                app_id: &app_id,
                size: PixelSize {
                    width: options.width,
                    height: options.height,
                },
                buffer_budget: options.buffer_budget,
            })?;
        let root = ui.root();
        runtime.windows.push(Entry {
            id,
            ui,
            seat: None,
            modifiers: Modifiers::default(),
            ready: false,
            #[cfg(feature = "unix-accessibility")]
            title: title.into(),
            #[cfg(feature = "unix-accessibility")]
            accessibility: aegle_access::UnixAdapter::new(move || wake.wake()),
            #[cfg(feature = "unix-accessibility")]
            initial_access: false,
        });
        Ok(Window {
            runtime: Rc::downgrade(&self.runtime),
            id,
            root,
        })
    }

    /// Whether this connection supports native text-input-v3 composition.
    /// Focusing an editable field without it returns `ImeUnavailable` from run.
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
        let index = runtime
            .windows
            .iter()
            .position(|entry| entry.id == self.id)
            .ok_or(UiError::DeadHandle)?;
        runtime.backend.remove_window(self.id)?;
        runtime.windows[index].ui.close()?;
        runtime.windows.remove(index);
        Ok(())
    }

    /// Applies a new theme to this window without replacing retained controls.
    pub fn set_theme(&self, theme: Theme) -> Result<()> {
        self.ui()?.set_theme(theme)
    }

    /// Sets this window's explicit reduced-motion preference. Enabling it snaps
    /// active paint transitions to their targets without changing focus or text.
    #[cfg(feature = "motion")]
    pub fn set_reduced_motion(&self, reduced: bool) -> Result<()> {
        self.ui()?.set_reduced_motion(reduced)
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
