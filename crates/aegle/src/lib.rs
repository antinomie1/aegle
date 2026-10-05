//! Convenient entry to Aegle's retained application layer.
//!
//! The current default desktop host is Linux Wayland with software rendering,
//! system fonts and Unix accessibility. No GPU or other native platform support
//! is implied by the default feature name. Lower-level crates remain independent.

pub use aegle_app::*;
extern crate self as aegle;

/// Compile a `.aegle` file to ordinary retained control construction.
#[cfg(feature = "markup")]
pub use aegle_macros::ui;

/// Common imperative application and control types.
pub mod prelude {
    #[cfg(all(feature = "wayland", target_os = "linux"))]
    pub use aegle_app::{App, AppOptions, Window, WindowOptions};
    pub use aegle_app::{Button, Container, Label, Node, Result, TextField, Theme, Ui};
}
