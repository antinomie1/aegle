//! The target's system accessibility adapter for one window, keeping native
//! handles owned through teardown. This and `Native` are the only
//! per-platform choices in the host.
#![allow(unsafe_code)]

use crate::Native;
use aegle_platform::{WindowId, WindowInfo};
use aegle_ui::{Result, Ui};

#[cfg(target_os = "linux")]
type Adapter = aegle_access::UnixAdapter;
#[cfg(target_os = "windows")]
type Adapter = aegle_access::WindowsAdapter<crate::native_render::Surface>;
#[cfg(target_os = "macos")]
type Adapter = aegle_access::MacosAdapter<crate::native_render::Surface>;

pub(crate) struct Access {
    adapter: Adapter,
    /// The adapter asked for the whole tree.
    initial: bool,
    /// Scale of the bounds the adapter reports: UI Automation and
    /// NSAccessibility use physical pixels, AT-SPI window-local logical ones.
    scale: f64,
}

impl Access {
    pub fn new(platform: &mut Native, window: WindowId) -> Result<Self> {
        use aegle_platform::Platform;
        let wake = platform.wake_handle()?;
        #[cfg(target_os = "linux")]
        let adapter = {
            let _ = window;
            Adapter::new(move || wake.wake())
        };
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let adapter = {
            use aegle_platform::SurfacePlatform;
            let surface = platform.window_surface(window)?;
            // SAFETY: the owned lease keeps this native view (or HWND) alive
            // until the adapter has removed its subclass during destruction.
            // The window is still hidden and unfocused here, as NSView
            // subclassing requires.
            unsafe { Adapter::new(surface, move || wake.wake())? }
        };
        Ok(Self {
            adapter,
            initial: false,
            scale: 1.0,
        })
    }

    /// Follows the window's activation and scale.
    pub fn configure(&mut self, info: &WindowInfo) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        self.adapter.set_window_focused(info.active);
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if self.scale != f64::from(info.scale) {
            // Bounds and hit testing are in physical pixels, so a new scale
            // needs the whole tree again.
            self.scale = f64::from(info.scale);
            self.initial = true;
        }
    }

    /// The next request from an assistive technology; a request for the
    /// whole tree is handled here.
    pub fn next_action(&mut self) -> Option<Option<aegle_access::accesskit::ActionRequest>> {
        Some(match self.adapter.next_event()? {
            aegle_access::Event::InitialTree => {
                self.initial = true;
                None
            }
            aegle_access::Event::Action(action) => Some(action),
            aegle_access::Event::Deactivate => None,
        })
    }

    /// Sends the window's semantic changes, or the whole tree when asked.
    pub fn publish(&mut self, ui: &Ui, title: &str) -> Result<()> {
        if std::mem::take(&mut self.initial) || ui.access_dirty() {
            let adapter = &mut self.adapter;
            ui.publish_accessibility(title, self.scale, |build| {
                adapter.update_if_active(build);
            })?;
        }
        Ok(())
    }
}
