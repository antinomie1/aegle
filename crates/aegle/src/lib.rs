//! Convenient entry to Aegle's retained application layer.
//!
//! The default desktop host selects Wayland on Linux or Win32 on Windows with
//! software rendering and system fonts. System accessibility adapters are
//! opt-in through `unix-accessibility` and `windows-accessibility`. Enable `vulkan`
//! or `wgpu` and select `RendererBackend::Vulkan` or `RendererBackend::Wgpu` for GPU presentation. Lower-level crates
//! remain independent; no macOS host is currently implemented.

pub use aegle_app::*;
/// Bounded PNG decoding into scene images, e.g. `ImageView` sources.
pub use aegle_glyph::{DecodeError, DecodedImage, decode_image, decode_png};
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
        Appearance, Button, Canvas, CheckBox, Color, Container, ControlKind, Cursor, Dropdown,
        ImageView, Label, ListView, Node, Point, Popup, Progress, Radio, Result, ScrollView, Skin,
        Slider, Style, Switch, Table, TableColumn, TextField, Theme, Ui, VisualState,
    };
    #[cfg(feature = "motion")]
    pub use aegle_app::{Easing, Transition};
}
