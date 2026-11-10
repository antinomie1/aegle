//! Native application host: [`App`] owns the platform connection, windows, event
//! loop, renderers and system preferences, and gives each window an
//! `aegle-ui` [`Ui`](aegle_ui::Ui). `native` hosts the target's platform crate:
//! Win32 on Windows and AppKit on macOS, and on Linux `sdl3` (windows through
//! the distribution's SDL3) or `wayland` (the native Wayland client); a Linux
//! build with neither still compiles and fails when the application connects,
//! naming the missing feature. It comes with one renderer feature
//! (`tiny-skia`, `cairo`, `vulkan` or `wgpu`).
//!
//! The `aegle` facade's defaults give way to an explicit choice: `wayland`
//! replaces `sdl3`, and `cairo`, `vulkan` or `wgpu` replaces `tiny-skia`, so
//! adding one feature switches the backend. The replaced crate is compiled but
//! never linked, so a binary carries no backend it cannot use; two of
//! `cairo`, `vulkan` and `wgpu` are a compile error. [`RENDERER`] names the
//! renderer of the build. `vulkan` has no macOS backend and fails at `App`
//! creation there, where `wgpu` presents through Metal.
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
// can never select. tiny-skia, the default, gives way to the one named
// alongside it (see `build.rs`); two others conflict.
#[cfg(any(
    all(feature = "cairo", any(feature = "vulkan", feature = "wgpu")),
    all(feature = "vulkan", feature = "wgpu"),
))]
compile_error!(
    "aegle-app: enable one renderer feature besides the default tiny-skia \
     (cairo, vulkan or wgpu), not two."
);

#[cfg(native)]
mod event_clock;
#[cfg(all(
    native,
    target_os = "linux",
    not(any(feature = "sdl3", feature = "wayland"))
))]
mod missing_backend;
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
mod native_manage;
#[cfg(native)]
mod native_proxy;
#[cfg(native)]
mod native_render;
#[cfg(native)]
mod native_shell;
#[cfg(native)]
mod renderer;

// A Linux build uses exactly one window backend; the others pick their
// target's own. `sdl3`, the default, gives way to `wayland`, whose crate is
// then compiled but never linked.
/// The target's platform; a new platform adds one line.
#[cfg(all(
    native,
    target_os = "linux",
    feature = "sdl3",
    not(feature = "wayland")
))]
type Native = aegle_platform_sdl3::Sdl3;
#[cfg(all(
    native,
    target_os = "linux",
    not(any(feature = "sdl3", feature = "wayland"))
))]
type Native = missing_backend::Missing;
#[cfg(all(native, target_os = "linux", feature = "wayland"))]
type Native = aegle_platform_wayland::Wayland;
#[cfg(all(native, target_os = "windows"))]
type Native = aegle_platform_win32::Win32;
#[cfg(all(native, target_os = "macos"))]
type Native = aegle_platform_macos::MacOs;

#[cfg(feature = "desktop-services")]
pub use aegle_desktop as desktop;
pub use aegle_platform::{
    Anchor, Icon, KeyboardInteractivity, Layer, LayerOptions, OutputId, OutputInfo, Preferences,
    Side, WindowMode,
};
#[cfg(feature = "vulkan")]
pub use aegle_render_vulkan::{
    Options as VulkanOptions, RawDevice, SharedDevice, SharedGlyphCache, ash,
};
#[cfg(feature = "wgpu")]
pub use aegle_render_wgpu::{Options as WgpuOptions, SharedGpu, wgpu};
#[cfg(native)]
pub use native::{App, AppOptions, Window, WindowOptions};
#[cfg(native)]
pub use native_proxy::UiProxy;
#[cfg(native)]
pub use native_shell::PopupOptions;
#[cfg(native)]
pub use renderer::RENDERER;
