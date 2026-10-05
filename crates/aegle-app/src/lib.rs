//! Retained imperative controls sharing layout, input, painting and semantics.
//!
//! [`Ui`] works without a window or renderer. It owns the retained tree; control
//! handles are weak references and dropping a handle does not remove a node.
//! Native applications enable `wayland` on Linux or `windows` on Windows, plus
//! an explicit `software`, `vulkan` or `wgpu` renderer feature for `App`.
//! `system-fonts` adds system discovery; explicit fonts remain available without
//! it. `accessibility` exports semantic trees, while `unix-accessibility` also
//! connects them to AT-SPI; `windows-accessibility` connects Windows UI Automation.
//! Both renderers consume the same retained scenes, input and editor state.
//! [`scene`] re-exports the drawing commands used by [`Canvas`] painters and images.

#[cfg(feature = "accessibility")]
mod access_scroll;
#[cfg(feature = "accessibility")]
mod accessibility;
mod callbacks;
mod handles;
mod input;
mod layout;
mod list;
#[cfg(feature = "motion")]
mod motion;
#[cfg(feature = "motion")]
mod motion_handles;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native;
#[cfg(any(
    all(feature = "unix-accessibility", target_os = "linux"),
    all(feature = "windows-accessibility", target_os = "windows")
))]
mod native_access;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
#[cfg_attr(target_os = "windows", path = "native_input_windows.rs")]
mod native_input;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native_loop;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
mod native_render;
#[cfg(all(feature = "wayland", target_os = "linux"))]
use aegle_platform_wayland as platform;
#[cfg(all(feature = "windows", target_os = "windows"))]
use aegle_platform_win32 as platform;
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
mod ui;
mod value_handles;
mod visual_handles;
mod widget_paint;

pub use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
#[cfg(feature = "motion")]
pub use aegle_motion::{Easing, Transition};
#[cfg(feature = "vulkan")]
pub use aegle_render_vulkan::Options as VulkanOptions;
#[cfg(feature = "wgpu")]
pub use aegle_render_wgpu::Options as WgpuOptions;
pub use aegle_scene as scene;
pub use aegle_text::{ImeEdit, Selection, TextSystem};
pub use aegle_theme::{Appearance, ControlKind, Skin, Style, Theme, VisualState};
pub use aegle_types::{Color, Point, Size};
pub use handles::{Button, Container, Label, Node, TextField};
pub use list::ListView;
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub use native::{App, AppOptions, RendererBackend, Window, WindowOptions};
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub use platform::Preferences;
#[cfg(all(feature = "wayland", target_os = "linux"))]
pub use platform::{Anchor, KeyboardInteractivity, Layer, LayerOptions};
pub use popup::{Dropdown, Popup};
pub use scroll_handles::ScrollView;
pub use table::{Table, TableColumn};
pub use ui::{ClipboardRequest, ImeRequest, ImeState, Result, Ui, UiError};
pub use value_handles::{CheckBox, Progress, Radio, Slider, Switch};
pub use visual_handles::{Canvas, ImageView};
