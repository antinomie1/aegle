use crate::{
    DragData, Error, ImeEvent, Key, Modifiers, Point, PointerButton, Preferences, SeatId,
    TouchPhase, WindowId, WindowInfo,
};

/// Native events in dispatch order. Positions are window-local logical
/// coordinates; times are platform milliseconds from an unspecified base,
/// wrapping at `u32::MAX`, comparable within one window.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event {
    /// A [`crate::WakeHandle`] signalled; drain the host's work queue.
    /// Several wakes may coalesce into one event.
    Wake,
    /// Geometry, scale or activation changed.
    Configure {
        /// Target window.
        window: WindowId,
        /// Latest state.
        info: WindowInfo,
    },
    /// The window can present one frame; requests coalesce until it does.
    Redraw {
        /// Target window.
        window: WindowId,
    },
    /// The user asked to close the window; the host decides whether to remove it.
    Close {
        /// Target window.
        window: WindowId,
    },
    /// A seat's keyboard focus entered or left the window.
    KeyboardFocus {
        /// Target window.
        window: WindowId,
        /// Originating seat.
        seat: SeatId,
        /// Whether focus entered.
        focused: bool,
    },
    /// A key changed state. Keys an input method consumes are not reported.
    Key {
        /// Window with the seat's keyboard focus.
        window: WindowId,
        /// Originating seat.
        seat: SeatId,
        /// Logical key identity, independent of produced text.
        key: Key,
        /// Text the press produces, when the platform translates it with the
        /// key; platforms that report text separately send [`Self::Text`].
        text: Option<String>,
        /// Press versus release.
        pressed: bool,
        /// An automatic repeat of a held key.
        repeat: bool,
        /// Modifiers for shortcuts; AltGr never reports Control.
        modifiers: Modifiers,
        /// Event time.
        time: u32,
    },
    /// Keyboard text produced apart from key events, including dead-key
    /// composition but excluding input-method edits.
    Text {
        /// Window with the seat's keyboard focus.
        window: WindowId,
        /// Originating seat.
        seat: SeatId,
        /// Complete Unicode text.
        text: String,
    },
    /// The seat's modifiers changed, including during a pointer gesture.
    Modifiers {
        /// Window with the seat's keyboard or pointer focus.
        window: WindowId,
        /// Originating seat.
        seat: SeatId,
        /// Current state.
        modifiers: Modifiers,
    },
    /// A mouse, touchpad or pen pointer event.
    Pointer {
        /// Window under or capturing the pointer.
        window: WindowId,
        /// Originating seat.
        seat: SeatId,
        /// Window-local logical position.
        position: Point,
        /// Transition.
        kind: PointerKind,
        /// Event time; `None` where the platform reports none, such as entry.
        time: Option<u32>,
    },
    /// One finger of a touch screen.
    Touch {
        /// Window the finger touched down in.
        window: WindowId,
        /// Originating seat.
        seat: SeatId,
        /// Finger identity among the seat's active fingers, reused after
        /// `Up` or `Cancel`.
        finger: u32,
        /// Window-local logical position.
        position: Point,
        /// Event time.
        time: u32,
        /// Contact stage.
        phase: TouchPhase,
    },
    /// A composition focus change or transaction for the window's editor.
    Ime {
        /// Target window.
        window: WindowId,
        /// Originating seat; sessions are seat-specific.
        seat: SeatId,
        /// Focus change or edit.
        event: ImeEvent,
    },
    /// Text a [`crate::Platform::request_clipboard`] call asked for.
    Clipboard {
        /// Requesting window.
        window: WindowId,
        /// Seat whose clipboard was read.
        seat: SeatId,
        /// Complete UTF-8 text.
        text: String,
    },
    /// A drag entered or moved over the window. Answer with
    /// [`crate::Platform::accept_drag`].
    Drag {
        /// Window under the drag.
        window: WindowId,
        /// Seat performing the drag.
        seat: SeatId,
        /// Window-local logical position.
        position: Point,
    },
    /// The drag left the window, or its drop was refused or unreadable.
    DragLeave {
        /// Window the drag left.
        window: WindowId,
        /// Seat performing the drag.
        seat: SeatId,
    },
    /// Data was dropped on a window that accepted the drag.
    Drop {
        /// Target window.
        window: WindowId,
        /// Seat performing the drag.
        seat: SeatId,
        /// Window-local logical drop position.
        position: Point,
        /// Dropped text or files.
        data: DragData,
    },
    /// System appearance preferences changed; not tied to a window.
    Preferences(Preferences),
    /// An asynchronous native failure; no panic crosses into native code.
    Error(Error),
}

impl Event {
    /// The window the event concerns, if any.
    pub fn window(&self) -> Option<WindowId> {
        match self {
            Self::Configure { window, .. }
            | Self::Redraw { window }
            | Self::Close { window }
            | Self::KeyboardFocus { window, .. }
            | Self::Key { window, .. }
            | Self::Text { window, .. }
            | Self::Modifiers { window, .. }
            | Self::Pointer { window, .. }
            | Self::Touch { window, .. }
            | Self::Ime { window, .. }
            | Self::Clipboard { window, .. }
            | Self::Drag { window, .. }
            | Self::DragLeave { window, .. }
            | Self::Drop { window, .. } => Some(*window),
            Self::Wake | Self::Preferences(_) | Self::Error(_) => None,
        }
    }
}

/// A pointer transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerKind {
    /// Entry or motion, including captured motion outside the window.
    Move,
    /// Primary button pressed.
    Down {
        /// The platform's own consecutive-click count (1 when it keeps none);
        /// hosts may count more.
        clicks: u8,
    },
    /// Primary button released.
    Up,
    /// Another button pressed.
    ButtonDown(PointerButton),
    /// Another button released.
    ButtonUp(PointerButton),
    /// The pointer left, or capture was lost: cancel the current gesture.
    Leave,
    /// Wheel or touchpad scrolling in logical pixels, positive right/down,
    /// with the system's scroll settings applied.
    Scroll {
        /// Displacement.
        delta: Point,
    },
    /// Fingers lifted from a touchpad scroll; kinetic scrolling may continue.
    ScrollEnd,
}
