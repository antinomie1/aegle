//! Native application host: [`App`] owns the platform connection, windows, event
//! loop, renderers and system preferences, and gives each window an
//! `aegle-ui` [`Ui`](aegle_ui::Ui). On Linux enable `wayland`, on Windows
//! `windows`, on macOS `macos`, plus an explicit `software`, `vulkan` or `wgpu`
//! renderer feature (macOS presents GPU frames through Metal, so `vulkan` is
//! unavailable there).
//! Windows share fonts, the platform connection and (for GPU renderers) one
//! device; each keeps its own control tree. [`App::proxy`] posts messages from
//! other threads to the UI thread. `unix-accessibility`,
//! `windows-accessibility` and `macos-accessibility` connect the semantic tree
//! to the system; the
//! controls' own roles and names additionally need `aegle-widgets/accessibility`,
//! which the `aegle` facade's adapter features enable.
//!
//! The host is written once against the [`aegle_platform::Platform`] contract;
//! the target's platform is chosen in one place, `Native` below.

#[cfg(native)]
mod event_clock;
#[cfg(native)]
mod native;
#[cfg(access)]
mod native_access;
#[cfg(native)]
mod native_files;
#[cfg(native)]
mod native_input;
#[cfg(native)]
mod native_loop;
#[cfg(native)]
mod native_proxy;
#[cfg(native)]
mod native_render;

/// The target's platform; a new platform adds one line.
#[cfg(all(native, target_os = "linux"))]
type Native = aegle_platform_wayland::Wayland;
#[cfg(all(native, target_os = "windows"))]
type Native = aegle_platform_win32::Win32;
#[cfg(all(native, target_os = "macos"))]
type Native = aegle_platform_macos::MacOs;

#[cfg(feature = "desktop-services")]
pub use aegle_desktop as desktop;
pub use aegle_platform::{Anchor, KeyboardInteractivity, Layer, LayerOptions, Preferences};
#[cfg(feature = "vulkan")]
pub use aegle_render_vulkan::{Options as VulkanOptions, RawDevice, SharedDevice, ash};
#[cfg(feature = "wgpu")]
pub use aegle_render_wgpu::{Options as WgpuOptions, SharedGpu, wgpu};
#[cfg(native)]
pub use native::{App, AppOptions, RendererBackend, Window, WindowOptions};
#[cfg(native)]
pub use native_proxy::UiProxy;
