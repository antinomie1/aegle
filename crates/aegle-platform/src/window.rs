use std::sync::Arc;

use crate::{Point, Rect};

/// A window's identity on its platform connection; never reused by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

/// An input seat: a keyboard, pointer and touch group with its own focus,
/// clipboard and composition. Platforms with one seat use `SeatId(0)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeatId(pub u32);

/// A whole-pixel extent; each use says whether it is logical or physical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelSize {
    /// Horizontal extent.
    pub width: u32,
    /// Vertical extent.
    pub height: u32,
}

/// Settings for a new window.
#[derive(Clone, Copy, Debug)]
pub struct WindowOptions<'a> {
    /// Human-readable title, at most 4000 UTF-8 bytes without NUL.
    pub title: &'a str,
    /// Reverse-DNS application identifier, at most 4000 UTF-8 bytes without
    /// NUL; Wayland uses it for desktop matching and as a layer's namespace.
    pub app_id: &'a str,
    /// Preferred logical size; the window system may override it.
    pub size: PixelSize,
    /// Most bytes of software presentation storage the window may hold.
    pub buffer_budget: usize,
    /// A desktop-shell layer such as a panel or overlay instead of an ordinary
    /// window; [`crate::Error::Unsupported`] where the platform has no layers.
    pub layer: Option<LayerOptions>,
    /// A popup attached to another window of this connection, such as a menu
    /// or a tooltip, instead of an ordinary window; `size` is its preferred
    /// extent. [`crate::Error::Unsupported`] where the platform has no
    /// popups. Setting both `layer` and `popup` is [`crate::Error::InvalidRole`].
    pub popup: Option<PopupOptions>,
    /// The renderer presents per-pixel alpha that the window system composites
    /// with what lies behind the window. Win32 then gives the window no
    /// redirection surface, so only a DirectComposition presenter shows it.
    pub transparent: bool,
}

impl Default for WindowOptions<'_> {
    fn default() -> Self {
        Self {
            title: "Aegle",
            app_id: "org.aegle.app",
            size: PixelSize {
                width: 800,
                height: 480,
            },
            buffer_budget: usize::MAX,
            layer: None,
            popup: None,
            transparent: false,
        }
    }
}

/// Placement of a desktop-shell layer surface, such as a panel or overlay.
///
/// Anchoring both opposite edges stretches that axis to the output; otherwise
/// the preferred size applies. Everything but `output` can change while the
/// surface lives, through [`crate::Platform::set_layer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayerOptions {
    /// Stacking layer.
    pub layer: Layer,
    /// Edges the surface attaches to; empty centers it.
    pub anchor: Anchor,
    /// Logical pixels reserved along an anchored edge; -1 ignores other zones.
    pub exclusive_zone: i32,
    /// Keyboard focus policy.
    pub keyboard: KeyboardInteractivity,
    /// Logical distance from the anchored edges, in CSS order: top, right,
    /// bottom, left. Edges that are not anchored ignore theirs.
    pub margin: [i32; 4],
    /// The output the surface is shown on, from [`crate::Platform::outputs`];
    /// `None` lets the compositor choose, usually the focused output. Fixed
    /// for the surface's life.
    pub output: Option<OutputId>,
}

/// Stacking order of a layer surface, bottom to top.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    /// Below everything, like a wallpaper.
    Background,
    /// Below ordinary windows.
    Bottom,
    /// Above ordinary windows, like a panel.
    #[default]
    Top,
    /// Above everything, including fullscreen windows.
    Overlay,
}

/// The output edges a layer surface attaches to; combine with `|`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anchor(u8);

impl Anchor {
    /// No edge: the surface is centered.
    pub const NONE: Self = Self(0);
    /// The top edge.
    pub const TOP: Self = Self(1);
    /// The bottom edge.
    pub const BOTTOM: Self = Self(2);
    /// The left edge.
    pub const LEFT: Self = Self(4);
    /// The right edge.
    pub const RIGHT: Self = Self(8);

    /// Whether every edge of `other` is included.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Anchor {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Whether a layer surface takes keyboard focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardInteractivity {
    /// Never focused, like a status panel.
    #[default]
    None,
    /// Takes all keyboard input while shown, like a lock screen.
    Exclusive,
    /// Focused when clicked, like an ordinary window.
    OnDemand,
}

/// A popup's attachment to its parent window.
///
/// The popup is placed against `anchor` on the `side` it names, and
/// the window system flips it to the opposite side or slides it along the
/// edge when it would leave the screen. A popup closes ([`crate::Event::Close`])
/// when the window system dismisses it; remove it before its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupOptions {
    /// The window the popup belongs to: an ordinary window, a layer or
    /// another popup.
    pub parent: WindowId,
    /// The rectangle of the parent's content it attaches to, in the parent's
    /// logical coordinates; its extent must be positive.
    pub anchor: Rect,
    /// The side of `anchor` the popup opens on.
    pub side: Side,
    /// Takes an explicit grab for this seat's latest press, as menus do: the
    /// popup receives the seat's keyboard focus and a press outside the
    /// popup's chain dismisses it. `None` makes a passive popup, like a
    /// tooltip, that never takes focus.
    pub grab: Option<SeatId>,
}

/// The side of its anchor a popup opens on, its start aligned with the
/// anchor's start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// Below the anchor, left edges aligned, like a dropdown menu.
    #[default]
    Below,
    /// Above the anchor, left edges aligned.
    Above,
    /// Left of the anchor, top edges aligned.
    Left,
    /// Right of the anchor, top edges aligned, like a submenu.
    Right,
}

/// An output's identity on its platform connection, valid while the output
/// is connected; a reconnected monitor may get a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputId(pub u64);

/// A connected display output, as [`crate::Platform::outputs`] lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputInfo {
    /// Identity for [`LayerOptions::output`].
    pub id: OutputId,
    /// The window system's name for the output, such as `DP-1` or
    /// `HDMI-A-1` on Wayland; empty when it reports none.
    pub name: String,
    /// Human-readable make and model; empty when unreported.
    pub description: String,
    /// Logical position of the top-left corner in the desktop; some window
    /// systems report the origin for every output.
    pub position: Point,
    /// Logical extent, the space a layer surface lays out in.
    pub size: PixelSize,
    /// Physical pixels per logical pixel, possibly fractional.
    pub scale: f32,
}

/// A window's latest configured geometry and activation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowInfo {
    /// Logical content extent, the size the interface lays out in.
    pub size: PixelSize,
    /// Physical framebuffer extent, the size presenters draw.
    pub buffer: PixelSize,
    /// Physical pixels per logical pixel, possibly fractional; multiply
    /// logical drawing coordinates by this.
    pub scale: f32,
    /// Whether the window is the active one; always true for layers.
    pub active: bool,
    /// Whether the window can present: configured and not minimized.
    pub configured: bool,
}

/// Cloneable, thread-safe signal that queues [`crate::Event::Wake`] on the
/// platform's loop. Wakes coalesce; store work in a host queue first. It keeps
/// only its signal alive, never the platform or its windows.
#[derive(Clone)]
pub struct WakeHandle(Arc<dyn Fn() + Send + Sync>);

impl WakeHandle {
    /// Wraps a platform's native signal.
    pub fn new(signal: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(signal))
    }

    /// Wakes the loop. Has no effect after the platform is destroyed.
    pub fn wake(&self) {
        (self.0)()
    }
}

impl std::fmt::Debug for WakeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WakeHandle")
    }
}
