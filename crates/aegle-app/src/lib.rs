//! Native application host: [`App`] owns the platform connection, windows, event
//! loop, renderers and system preferences, and gives each window an
//! `aegle-ui` [`Ui`](aegle_ui::Ui). `native` builds the target's platform crate
//! (Wayland, Win32 or AppKit; `wayland`, `windows` and `macos` select one
//! explicitly), plus an explicit `software`, `vulkan` or `wgpu` renderer
//! feature. `vulkan` has no macOS backend and returns `Unsupported` there,
//! where `wgpu` presents through Metal.
//!
//! Windows share fonts, the platform connection and (for GPU renderers) one
//! device; each keeps its own control tree. [`App::proxy`] posts messages from
//! other threads to the UI thread. `accessibility` connects the semantic tree
//! to the target's system adapter (AT-SPI, UI Automation or
//! NSAccessibility) and enables `aegle-ui/accessibility`; the controls' own
//! roles and names additionally need `aegle-widgets/accessibility`, which the
//! `aegle` facade's `accessibility` feature enables.
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
