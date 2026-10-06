//! Stateless default-skin painters and scroll geometry for retained controls.
//!
//! Everything here is a pure function from a control's size, state and
//! [`aegle_theme::Appearance`] to scene commands or geometry, so a host that
//! builds its own tree on `aegle-controls` can reuse the default look without
//! `aegle-app`. Behavior that needs the retained tree (focus, hit testing,
//! popups, virtual lists) stays in the application layer.
#[cfg(feature = "effects")]
pub mod effects;
mod paint;
mod scroll;
pub mod scrollbar;

pub use paint::{CHEVRON, Mark, ToggleSpec, check_mark, chevron, range, slider_track, toggle};
pub use scroll::{clamp_anchor, intersection, reveal_delta};
pub use scrollbar::Bar;
