//! Convenient entry to Aegle's retained application layer.
//!
//! The default desktop host selects Wayland on Linux or Win32 on Windows with
//! software rendering, system fonts and native accessibility. Enable `vulkan`
//! and select `RendererBackend::Vulkan` for GPU presentation. Lower-level crates
//! remain independent; no macOS host is currently implemented.

pub use aegle_app::*;
extern crate self as aegle;

/// Compile a `.aegle` file to ordinary retained control construction.
#[cfg(feature = "markup")]
pub use aegle_macros::ui;

/// Runtime engine for dynamic markup and run-time loading.
#[cfg(feature = "markup")]
pub use aegle_loader as loader;

/// Common imperative application and control types.
pub mod prelude {
    #[cfg(any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    pub use aegle_app::{App, AppOptions, RendererBackend, Window, WindowOptions};
    pub use aegle_app::{
        Appearance, Button, Canvas, CheckBox, Color, Container, ControlKind, ImageView, Label,
        ListView, Node, Point, Progress, Result, ScrollView, Skin, Slider, Style, Switch,
        TextField, Theme, Ui, VisualState,
    };
    #[cfg(feature = "motion")]
    pub use aegle_app::{Easing, Transition};
}
