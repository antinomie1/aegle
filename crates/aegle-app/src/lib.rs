//! Native application host: [`App`] owns the platform connection, windows, event
//! loop, renderers and system preferences, and gives each window an
//! `aegle-ui` [`Ui`](aegle_ui::Ui). `native` builds the target's platform crate
//! (Wayland, Win32 or AppKit; `wayland`, `windows` and `macos` select one
//! explicitly), plus exactly one renderer feature (`tiny-skia`, `cairo`,
//! `vulkan` or `wgpu`). The renderer features are mutually exclusive: enabling
//! two is a compile error, so a binary never carries a backend it cannot use,
//! and [`RENDERER`] names the one it has. `vulkan` has no macOS backend and
//! fails at `App` creation there, where `wgpu` presents through Metal.
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

// One renderer per build: a second backend would only add code the application
// can never select.
#[cfg(any(
    all(
        feature = "tiny-skia",
        any(feature = "cairo", feature = "vulkan", feature = "wgpu")
    ),
    all(feature = "cairo", any(feature = "vulkan", feature = "wgpu")),
    all(feature = "vulkan", feature = "wgpu"),
))]
compile_error!(
    "aegle-app: enable exactly one renderer feature (tiny-skia, cairo, vulkan or wgpu). \
     The `aegle` defaults include tiny-skia; build another renderer with \
     `--no-default-features --features desktop,<renderer>`."
);

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
#[cfg(native)]
mod renderer;

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
pub use native::{App, AppOptions, Window, WindowOptions};
#[cfg(native)]
pub use native_proxy::UiProxy;
#[cfg(native)]
pub use renderer::RENDERER;
