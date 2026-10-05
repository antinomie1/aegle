//! Retained imperative controls sharing layout, input, painting and semantics.
//!
//! [`Ui`] works without a window or renderer. It owns the retained tree; control
//! handles are weak references and dropping a handle does not remove a node.
//! Native Linux applications enable `wayland` for `App` and software windows.
//! `system-fonts` adds system discovery; explicit fonts remain available without
//! it. `accessibility` exports semantic trees, while `unix-accessibility` also
//! connects them to AT-SPI. Other native platforms and GPU hosts are not yet
//! implemented.

#[cfg(feature = "accessibility")]
mod accessibility;
mod callbacks;
mod handles;
mod input;
mod layout;
#[cfg(feature = "motion")]
mod motion;
#[cfg(feature = "motion")]
mod motion_handles;
#[cfg(all(feature = "wayland", target_os = "linux"))]
mod native;
#[cfg(all(feature = "wayland", target_os = "linux"))]
mod native_input;
#[cfg(all(feature = "wayland", target_os = "linux"))]
mod native_loop;
mod paint;
mod state;
mod style;
mod style_handles;
mod text_handles;
mod theme;
mod ui;

pub use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
#[cfg(feature = "motion")]
pub use aegle_motion::{Easing, Transition};
pub use aegle_text::{ImeEdit, TextSystem};
pub use aegle_theme::{Appearance, ControlKind, Skin, Style, Theme, VisualState};
pub use aegle_types::{Color, Point, Size};
pub use handles::{Button, Container, Label, Node, TextField};
#[cfg(all(feature = "wayland", target_os = "linux"))]
pub use native::{App, AppOptions, Window, WindowOptions};
pub use ui::{ImeRequest, ImeState, Result, Ui, UiError};
