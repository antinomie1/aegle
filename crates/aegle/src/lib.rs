//! Convenient entry to Aegle's retained application layer.
//!
//! The default desktop host selects Wayland on Linux or Win32 on Windows with
//! software rendering and system fonts. System accessibility adapters are
//! opt-in through `unix-accessibility` and `windows-accessibility`. Enable `vulkan`
//! or `wgpu` and select `RendererBackend::Vulkan` or `RendererBackend::Wgpu` for GPU presentation. Lower-level crates
//! remain independent; no macOS host is currently implemented.
//!
//! [`prelude`] holds the common names; everything else stays under the crate
//! it comes from: [`ui`](mod@ui), [`widgets`] and [`app`].

/// Native application host: `App`, windows, renderer selection and desktop
/// services. Empty without a native platform or GPU renderer feature.
pub use aegle_app as app;
/// Bounded image decoding into scene images (PNG always; JPEG, WebP, GIF and SVG by
/// feature) and, with `effects`, gradient and shadow images.
pub use aegle_image as image;
/// The retained engine: `Ui`, `Node`, `Container`, styles, themes, input
/// types and the scene.
pub use aegle_ui as ui;
/// The default controls.
pub use aegle_widgets as widgets;

/// Declare markup elements for a control library.
#[cfg(feature = "markup")]
pub use aegle_loader::element;
#[doc(hidden)]
#[cfg(feature = "markup")]
pub use aegle_macros::{__ui, __ui_resume};

/// Compiles a manifest-relative `.aegle` file into a typed retained view.
///
/// `ui!("view.aegle")` produces a builder closure taking `&aegle::app::App` for a
/// `Window` root, or `&aegle::ui::Container` for any other component root. The builder
/// returns `aegle::ui::Result<View>`. `ui!(parent, "view.aegle")` invokes that builder
/// immediately and evaluates the parent expression once.
///
/// The inferred view has a public `root` handle and a public typed field for
/// every markup `id`, plus a `loader::State<T>` field for every state of a
/// dynamic document's root. Bind Rust callbacks through those fields after
/// creation; IDs inside blocks and components are not exposed. Imports resolve
/// against the importing file and are tracked for recompilation too.
/// Dropping the view keeps its retained controls alive. A construction failure
/// removes the new subtree; it never removes the parent supplied by the caller.
/// Transition properties require the `motion` feature. Transitions are
/// installed after every static property, so initial construction does not animate.
///
/// Element names resolve in Rust scope where `ui!` is called, like types:
/// `use aegle::prelude::*` brings the built-in elements, and a control
/// library's elements come with their handle types. Each element's spec
/// reaches the checker through the macro [`element!`] defines with its name,
/// so a library's elements are checked like the built-in ones.
///
/// Paths are relative to `CARGO_MANIFEST_DIR`, including explicit `../` paths.
/// The generated dependency marker makes file edits trigger recompilation.
/// Unknown properties and unsupported language constructs are rejected with
/// file, line and column diagnostics at the path argument; an element not in
/// scope is reported by rustc as a missing macro.
#[cfg(feature = "markup")]
#[macro_export]
macro_rules! ui {
    ($($input:tt)*) => {
        $crate::__ui!($crate; $($input)*)
    };
}

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
    /// The built-in markup elements: the specs `ui!` reads under the handle
    /// type names, and marker types for the other element names.
    #[cfg(feature = "markup")]
    pub use aegle_loader::{
        Button, CheckBox, Column, Grid, NumberField, Progress, RadioButton, Row, ScrollView,
        Separator, Slider, Splitter, Stack, Switch, Tab, Tabs, Text, TextArea, TextField,
    };
    pub use aegle_ui::{
        Align, Appearance, Color, ColorSlot, Container, ControlKind, Cursor, Direction, DragData,
        DropEvent, Font, Insets, Justify, LayoutDirection, Length, LengthSlot, Node, Point, Result,
        Shadow, Skin, Style, Theme, Token, TokenSlot, Ui, Visit, VisualState, Wrap, register_token,
    };
    #[cfg(feature = "motion")]
    pub use aegle_ui::{
        Animate, Animation, CubicBezier, Cycles, Easing, Keyframe, Spring, Transition,
        TransitionProperty,
    };
    #[cfg(feature = "grid")]
    pub use aegle_ui::{Flow, GridLine, GridLines, Placement, Repeat, TemplateItem, Track};
    pub use aegle_widgets::{
        Button, Canvas, CheckBox, Dropdown, ImageView, Label, ListView, MenuBar, MenuItem,
        NodeWidgets, NumberField, Orientation, Popup, Progress, Radio, RowHeight, Separator,
        Slider, Splitter, Switch, Table, TableColumn, Tabs, TextField, Widgets,
    };
}
