use crate::{Error, ImeEvent};
use aegle_types::Point;

/// Identity scoped to its event loop; never reused by that loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) u64);

/// Integer extent; see each field for logical versus physical units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelSize {
    /// Horizontal extent.
    pub width: u32,
    /// Vertical extent.
    pub height: u32,
}
impl PixelSize {
    pub(crate) fn bytes(self) -> Result<usize, Error> {
        if self.width == 0
            || self.height == 0
            || self.width > i32::MAX as u32 / 4
            || self.height > i32::MAX as u32
        {
            return Err(Error::InvalidSize);
        }
        (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(Error::InvalidSize)
    }
}

/// Initial opaque ordinary-window settings.
#[derive(Clone, Debug)]
pub struct WindowOptions<'a> {
    /// Human-readable native title.
    pub title: &'a str,
    /// Application identifier reserved for host integration; not a window class.
    pub app_id: &'a str,
    /// Preferred client size in logical pixels at 96 DPI.
    pub size: PixelSize,
    /// Maximum retained software pixel bytes. Default: 16 MiB.
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

/// Current client geometry; minimized windows do not receive redraws.
#[derive(Clone, Copy, Debug)]
pub struct WindowInfo {
    /// Logical client size rounded up from physical size / scale.
    pub size: PixelSize,
    /// Physical pixels per logical pixel; supports fractional Windows DPI.
    pub scale: f32,
    /// Actual keyboard activation state.
    pub active: bool,
    /// Whether the client extent is drawable (false when minimized).
    pub configured: bool,
    pub(crate) physical: PixelSize,
}
impl WindowInfo {
    /// Exact physical client extent; never reconstructed from rounded logical size.
    pub fn buffer_size(self) -> Result<PixelSize, Error> {
        self.physical.bytes()?;
        Ok(self.physical)
    }
}

/// Current logical key modifiers. AltGr remains the native Ctrl+Alt combination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Either Shift key.
    pub shift: bool,
    /// Either Control key.
    pub control: bool,
    /// Either Alt key.
    pub alt: bool,
    /// Either Windows key.
    pub meta: bool,
}

/// Primary pointer transitions in logical client coordinates.
#[derive(Clone, Copy, Debug)]
pub enum PointerKind {
    /// Motion or entry.
    Move,
    /// Primary down, including native double-click recognition.
    Down {
        /// Consecutive click count (1 or 2).
        clicks: u8,
    },
    /// Primary up.
    Up,
    /// Leave or loss of native capture; cancel the current gesture.
    Leave,
    /// Logical displacement, positive right/down. Native wheel settings apply.
    Scroll {
        /// Horizontal/vertical displacement.
        delta: Point,
    },
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
}

/// Native events in dispatch order; text and physical keys are distinct.
#[derive(Debug)]
pub enum Event {
    /// A producer has queued host work.
    Wake,
    /// Geometry, DPI or activation changed.
    Configure {
        /// Target.
        window: WindowId,
        /// Latest state.
        info: WindowInfo,
    },
    /// One frame is ready; repeated requests coalesce.
    Redraw {
        /// Target.
        window: WindowId,
    },
    /// The native titlebar requested closing; the host removes the window.
    Close {
        /// Target.
        window: WindowId,
    },
    /// Native keyboard focus changed.
    KeyboardFocus {
        /// Target.
        window: WindowId,
        /// New state.
        focused: bool,
    },
    /// Physical virtual-key transition; no translated text is duplicated here.
    Key {
        /// Target.
        window: WindowId,
        /// Win32 virtual-key code (VK_*).
        key: u32,
        /// Press versus release.
        pressed: bool,
        /// Automatic key repeat.
        repeat: bool,
        /// State at this event.
        modifiers: Modifiers,
    },
    /// Committed keyboard text (including dead-key composition), excluding IMM results.
    Text {
        /// Target.
        window: WindowId,
        /// Complete Unicode scalar or keyboard text.
        text: String,
    },
    /// Pointer event.
    Pointer {
        /// Target.
        window: WindowId,
        /// Logical position.
        position: Point,
        /// Transition.
        kind: PointerKind,
        /// State at this event.
        modifiers: Modifiers,
    },
    /// Native IMM compatibility composition update or focus transition.
    Ime {
        /// Target.
        window: WindowId,
        /// Transaction.
        event: ImeEvent,
    },
    /// System appearance preferences changed; not tied to a window.
    Preferences(Preferences),
    /// A native callback failed; no panic crosses the FFI boundary.
    Error(Error),
}
#[cfg(windows)]
impl Event {
    pub(crate) fn target(&self) -> Option<WindowId> {
        match self {
            Self::Configure { window, .. }
            | Self::Redraw { window }
            | Self::Close { window }
            | Self::KeyboardFocus { window, .. }
            | Self::Key { window, .. }
            | Self::Text { window, .. }
            | Self::Pointer { window, .. }
            | Self::Ime { window, .. } => Some(*window),
            _ => None,
        }
    }
}
