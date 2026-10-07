//! Native application host: [`App`] owns the platform connection, windows, event
//! loop, renderers and system preferences, and gives each window an
//! `aegle-ui` [`Ui`](aegle_ui::Ui). On Linux enable `wayland`, on Windows
//! `windows`, plus an explicit `software`, `vulkan` or `wgpu` renderer feature.
//! Windows share fonts, the platform connection and (for GPU renderers) one
//! device; each keeps its own control tree. [`App::proxy`] posts messages from
//! other threads to the UI thread. `unix-accessibility` and
//! `windows-accessibility` connect the semantic tree to the system; the
//! controls' own roles and names additionally need `aegle-widgets/accessibility`,
//! which the `aegle` facade's adapter features enable.

#[cfg(all(feature = "wayland", target_os = "linux"))]
use aegle_platform_wayland as platform;
#[cfg(all(feature = "windows", target_os = "windows"))]
use aegle_platform_win32 as platform;

#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod event_clock;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native;
#[cfg(any(
    all(feature = "unix-accessibility", target_os = "linux"),
    all(feature = "windows-accessibility", target_os = "windows")
))]
mod native_access;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
#[cfg_attr(target_os = "windows", path = "native_input_windows.rs")]
mod native_input;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native_loop;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native_proxy;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native_render;

#[cfg(feature = "vulkan")]
pub use aegle_render_vulkan::{Options as VulkanOptions, RawDevice, SharedDevice, ash};
#[cfg(feature = "wgpu")]
pub use aegle_render_wgpu::{Options as WgpuOptions, SharedGpu, wgpu};
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub use native::{App, AppOptions, RendererBackend, Window, WindowOptions};
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub use native_proxy::UiProxy;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub use platform::Preferences;
#[cfg(all(feature = "wayland", target_os = "linux"))]
pub use platform::{Anchor, KeyboardInteractivity, Layer, LayerOptions};
