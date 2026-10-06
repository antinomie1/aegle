//! Headless retained UI: [`Ui`] owns the control tree, layout, input routing,
//! focus, scrolling, motion, themes and the default controls, with no window,
//! renderer or operating system dependency.
//!
//! Control handles are weak references; dropping a handle does not remove a node.
//! `system-fonts` adds system font discovery (explicit fonts remain available
//! without it); `accessibility` exports semantic trees; `motion` adds
//! transitions, momentum scrolling and animated transforms. A native host such as
//! `aegle-app` or any embedding feeds input into a [`Ui`], calls [`Ui::refresh`]
//! and draws [`Ui::visit_scenes`] with the renderer of its choice.
//! [`scene`] re-exports the drawing commands used by `Canvas` painters and images.

#[cfg(feature = "accessibility")]
mod access_scroll;
#[cfg(feature = "accessibility")]
mod accessibility;
pub mod bar;
mod callbacks;
pub mod control;
mod cursor;
mod direction;
mod effects;
mod events;
#[cfg(feature = "motion")]
mod fling;
#[cfg(feature = "grid")]
mod grid_handles;
mod handles;
mod hover;
mod input;
mod layout;
mod layout_handles;
#[cfg(feature = "motion")]
mod motion;
#[cfg(feature = "motion")]
mod motion_handles;
pub mod paint;
mod scroll;
pub mod scroll_geometry;
mod scrollbar;
mod state;
mod style;
mod style_handles;
mod text;
mod theme;
mod token_handles;
mod tokens;
mod touch;
mod transform;
mod ui;

#[cfg(feature = "accessibility")]
pub use aegle_access::accesskit;
pub use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
pub use aegle_layout::{Align, Direction, Insets, Justify, LayoutDirection, Length, Wrap};
#[cfg(feature = "grid")]
pub use aegle_layout::{Flow, GridLine, GridLines, Placement, Repeat, TemplateItem, Track};
#[cfg(feature = "motion")]
pub use aegle_motion::{Easing, Transition};
pub use aegle_scene as scene;
pub use aegle_text::{ImeEdit, Selection, TextSystem};
pub use aegle_theme::{
    Appearance, ControlKind, Skin, Style, Theme, ThemeOverride, Token, TokenKind, TokenType,
    TokenValue, VisualState,
};
pub use aegle_types::{Color, Cursor, Point, Preferences, Size, TouchPhase};
pub use control::{Control, Plain};
pub use effects::Shadow;
pub use events::KeyEvent;
#[cfg(feature = "grid")]
pub use grid_handles::Stack;
pub use handles::{Container, Node, valid};
#[cfg(feature = "motion")]
pub use motion::TransitionProperty;
pub use state::{Element, Hooks, State, focus_policy, text_style};
pub use tokens::{ColorSlot, LengthSlot, StyleSlot, Tokens, register_token, token};

pub use transform::Transform;
pub use ui::{
    ClipboardRequest, ImeRequest, ImeState, Result, Ui, UiError, container_style, scroll_padding,
};
