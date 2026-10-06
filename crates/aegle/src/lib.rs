//! Convenient entry to Aegle's retained application layer.
//!
//! The default desktop host selects Wayland on Linux or Win32 on Windows with
//! software rendering and system fonts. System accessibility adapters are
//! opt-in through `unix-accessibility` and `windows-accessibility`. Enable `vulkan`
//! or `wgpu` and select `RendererBackend::Vulkan` or `RendererBackend::Wgpu` for GPU presentation. Lower-level crates
//! remain independent; no macOS host is currently implemented.

pub use aegle_app::*;
/// Bounded image decoding into scene images (PNG always; JPEG, WebP, GIF and SVG by
/// feature) and, with `effects`, gradient and shadow images.
pub use aegle_image as image;
pub use aegle_ui::*;
pub use aegle_widgets::*;
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
    pub use aegle_ui::{
        Align, Appearance, Color, Container, ControlKind, Cursor, Direction, Insets, Justify,
        Length, Node, Point, Result, Shadow, Skin, Style, Theme, Ui, VisualState, Wrap,
    };
    #[cfg(feature = "motion")]
    pub use aegle_ui::{Easing, Transition, TransitionProperty};
    #[cfg(feature = "grid")]
    pub use aegle_ui::{Flow, Placement, Track};
    pub use aegle_widgets::{
        Button, Canvas, CheckBox, Dropdown, ImageView, Label, ListView, NodePopup, NodeTooltip,
        NumberField, Orientation, Popup, Progress, Radio, ScrollView, Separator, Slider, Splitter,
        Switch, Table, TableColumn, Tabs, TextField, Widgets,
    };
}
