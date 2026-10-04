use crate::{Error, ImeEvent, KeyEvent, Modifiers, PointerEventKind, WlSeat};
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
        }
    }
}

/// Latest configured geometry and focus decoration state.
#[derive(Clone, Copy, Debug)]
pub struct WindowInfo {
    /// Logical surface extent.
    pub size: PixelSize,
    /// Integer buffer scale; multiply logical drawing coordinates by this.
    pub scale: u32,
    /// Whether the compositor marks this window active.
    pub active: bool,
    /// Whether an initial configure has arrived.
    pub configured: bool,
}

impl WindowInfo {
    /// Physical framebuffer extent, rejecting protocol overflow.
    pub fn buffer_size(self) -> Result<PixelSize, Error> {
        let width = self
            .size
            .width
            .checked_mul(self.scale)
            .ok_or(Error::InvalidSize)?;
        let height = self
            .size
            .height
            .checked_mul(self.scale)
            .ok_or(Error::InvalidSize)?;
        if width == 0 || width > i32::MAX as u32 / 4 || height == 0 || height > i32::MAX as u32 {
            return Err(Error::InvalidSize);
        }
        Ok(PixelSize { width, height })
    }
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
    /// A text-input-v3 batch or focus transition.
    Ime {
        /// Target window.
        window: WindowId,
        /// Originating seat; serials and composition sessions are seat-specific.
        seat: WlSeat,
        /// Protocol input event.
        event: ImeEvent,
    },
    /// Asynchronous input or protocol setup failure.
    Error(Error),
}
