use crate::{
    Anchor, Error, ImeEvent, KeyEvent, KeyboardInteractivity, Layer, Modifiers, PointerEventKind,
    WlSeat,
};
use aegle_types::Point;

/// Stable window identity, valid only on its originating connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) u64);

/// Non-fractional extent in either surface or buffer pixels, as documented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelSize {
    /// Horizontal extent.
    pub width: u32,
    /// Vertical extent.
    pub height: u32,
}

/// Initial ordinary-window settings.
#[derive(Clone, Debug)]
pub struct WindowOptions<'a> {
    /// Human-readable title, at most 4000 UTF-8 bytes and no NUL.
    pub title: &'a str,
    /// Desktop application identifier, at most 4000 UTF-8 bytes and no NUL.
    pub app_id: &'a str,
    /// Preferred logical size; the compositor may override it.
    pub size: PixelSize,
    /// Maximum live SHM mapping bytes for this window. Default: 16 MiB.
    pub buffer_budget: usize,
    /// Creates a wlr layer surface instead of a toplevel. Default: `None`.
    pub layer: Option<LayerOptions>,
}

/// Placement of a `zwlr_layer_shell_v1` surface, such as a panel or overlay.
///
/// Anchoring both opposite edges stretches that axis to the output; otherwise
/// the preferred size applies. `app_id` becomes the layer namespace.
#[derive(Clone, Copy, Debug)]
pub struct LayerOptions {
    /// Stacking layer.
    pub layer: Layer,
    /// Edges the surface attaches to; empty centers it.
    pub anchor: Anchor,
    /// Logical pixels reserved along an anchored edge; -1 ignores other zones.
    pub exclusive_zone: i32,
    /// Keyboard focus policy.
    pub keyboard: KeyboardInteractivity,
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
            buffer_budget: 16 * 1024 * 1024,
            layer: None,
        }
    }
}

/// Latest configured geometry and focus decoration state.
#[derive(Clone, Copy, Debug)]
pub struct WindowInfo {
    /// Logical surface extent.
    pub size: PixelSize,
    /// Scale from logical to buffer pixels, at least 1: whole numbers without
    /// fractional-scale support, otherwise the compositor's preferred scale.
    /// Multiply logical drawing coordinates by this.
    pub scale: f32,
    /// Whether the compositor marks this window active; always true for layers.
    pub active: bool,
    /// Whether an initial configure has arrived.
    pub configured: bool,
}

impl WindowInfo {
    /// Physical framebuffer extent, rejecting protocol overflow.
    pub fn buffer_size(self) -> Result<PixelSize, Error> {
        // Buffers are the logical size times the scale, rounded half away from zero.
        let scaled = |extent: u32| {
            let pixels = (f64::from(extent) * f64::from(self.scale)).round();
            (pixels >= 1.0 && pixels <= f64::from(i32::MAX / 4)).then_some(pixels as u32)
        };
        match (scaled(self.size.width), scaled(self.size.height)) {
            (Some(width), Some(height)) => Ok(PixelSize { width, height }),
            _ => Err(Error::InvalidSize),
        }
    }
}

/// Desktop appearance preferences; `None` where the system reports nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    /// Whether the user prefers a dark color scheme.
    pub dark: Option<bool>,
    /// Whether high-contrast presentation is requested.
    pub high_contrast: Option<bool>,
    /// Whether non-essential motion should be reduced.
    pub reduced_motion: Option<bool>,
    /// Text size as a percentage of the default (100), within 50–400.
    pub text_scale: Option<u16>,
}

/// Stage of one finger's contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    /// The finger touched the surface.
    Down,
    /// The finger moved while touching.
    Move,
    /// The finger lifted.
    Up,
    /// The compositor cancelled the contact, for example to take over a gesture.
    Cancel,
}

/// Native events in dispatch order. Coordinates use logical surface pixels.
#[derive(Debug)]
pub enum Event {
    /// An external producer requested a wake; drain the host's own work queue.
    /// Multiple requests may coalesce into one event.
    Wake,
    /// Geometry or activation changed.
    Configure {
        /// Target window.
        window: WindowId,
        /// Latest state.
        info: WindowInfo,
    },
    /// One frame can be drawn; requests coalesce until it is presented.
    Redraw {
        /// Target window.
        window: WindowId,
    },
    /// The compositor requested closing; the host decides when to remove it.
    Close {
        /// Target window.
        window: WindowId,
    },
    /// Keyboard focus changed for a seat.
    KeyboardFocus {
        /// Target window.
        window: WindowId,
        /// Originating seat.
        seat: WlSeat,
        /// Whether focus entered.
        focused: bool,
    },
    /// Physical key and XKB translation. IME-consumed keys are not duplicated.
    Key {
        /// Target window.
        window: WindowId,
        /// Originating seat.
        seat: WlSeat,
        /// XKB event including optional translated text.
        key: KeyEvent,
        /// Press versus release.
        pressed: bool,
        /// Server-configured key repeat.
        repeat: bool,
        /// Current modifier state.
        modifiers: Modifiers,
    },
    /// Modifier-only update, including changes during a pointer gesture.
    Modifiers {
        /// Window with keyboard focus.
        window: WindowId,
        /// Originating seat.
        seat: WlSeat,
        /// Current modifier state.
        modifiers: Modifiers,
    },
    /// Native pointer event; axis values retain SCTK protocol information.
    /// Losing a pointer capability synthesizes `Leave { serial: 0 }` to cancel
    /// host hover/capture; that serial must not authorize a protocol request.
    Pointer {
        /// Target window.
        window: WindowId,
        /// Originating seat.
        seat: WlSeat,
        /// Local logical position.
        position: Point,
        /// Motion, button, enter/leave or scroll details.
        kind: PointerEventKind,
    },
    /// One finger of a touch screen. `id` is unique among the seat's active fingers
    /// and is reused after `Up` or `Cancel`.
    Touch {
        /// Window the finger touched down in.
        window: WindowId,
        /// Originating seat.
        seat: WlSeat,
        /// Finger identity within the seat.
        id: i32,
        /// Local logical position.
        position: Point,
        /// Compositor time in milliseconds, for velocity.
        time: u32,
        /// Contact stage.
        phase: TouchPhase,
    },
    /// A text-input-v3 batch or focus transition.
    Ime {
        /// Target window.
        window: WindowId,
        /// Originating seat; serials and composition sessions are seat-specific.
        seat: WlSeat,
        /// Protocol input event.
        event: ImeEvent,
    },
    /// Text requested by [`crate::Wayland::request_clipboard`].
    Clipboard {
        /// Requesting window.
        window: WindowId,
        /// Seat whose selection was read.
        seat: WlSeat,
        /// Complete UTF-8 selection.
        text: String,
    },
    /// System appearance preferences changed; not tied to a window.
    Preferences(Preferences),
    /// Asynchronous input or protocol setup failure.
    Error(Error),
}
