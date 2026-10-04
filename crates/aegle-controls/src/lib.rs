//! Retained control behavior shared by custom skins and platform adapters.
//!
//! The host owns the logical tree, hit testing, focus and pointer capture.
//! Deliver local-coordinate [`Input`] to the target and apply its [`Outcome`].
//! No window, renderer, theme or event loop is required. Text editing is opt-in.

mod button;
mod input;
#[cfg(feature = "text")]
mod text_field;

pub use button::Button;
pub use input::{
    Action, Capture, Input, Key, KeyInput, Modifiers, Outcome, PointerId, PointerInput, PointerKind,
};
#[cfg(feature = "text")]
pub use text_field::TextField;
