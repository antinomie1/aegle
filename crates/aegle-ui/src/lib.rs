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
//! [`scene`] re-exports the drawing commands used by [`Canvas`] painters and images.

#[cfg(feature = "accessibility")]
mod access_scroll;
#[cfg(feature = "accessibility")]
mod accessibility;
mod callbacks;
mod cursor;
#[cfg(feature = "motion")]
mod fling;
mod handles;
mod input;
mod layout;
mod list;
#[cfg(feature = "motion")]
mod motion;
#[cfg(feature = "motion")]
mod motion_handles;
mod paint;
mod popup;
mod scroll;
mod scroll_handles;
mod scrollbar;
mod state;
mod style;
mod style_handles;
mod table;
mod text_handles;
mod theme;
mod touch;
mod transform;
mod ui;
mod value_handles;
mod visual_handles;

pub use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
#[cfg(feature = "motion")]
pub use aegle_motion::{Easing, Transition};
pub use aegle_scene as scene;
pub use aegle_text::{ImeEdit, Selection, TextSystem};
pub use aegle_theme::{Appearance, ControlKind, Skin, Style, Theme, ThemeOverride, VisualState};
pub use aegle_types::{Color, Cursor, Point, Size};
pub use handles::{Button, Container, Label, Node, TextField};
pub use list::ListView;
pub use popup::{Dropdown, Popup};
pub use scroll_handles::ScrollView;
pub use table::{Table, TableColumn};
pub use touch::TouchPhase;
pub use transform::Transform;
pub use ui::{ClipboardRequest, ImeRequest, ImeState, Result, Ui, UiError};
pub use value_handles::{CheckBox, Progress, Radio, Slider, Switch};
pub use visual_handles::{Canvas, ImageView};
