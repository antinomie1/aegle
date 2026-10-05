use aegle_types::Point;

/// Host-assigned identity of a pointer, stable for its lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerId(pub u64);

/// Modifiers relevant to control behavior; lock states are resolved by the platform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Extend a selection or use the shifted shortcut.
    pub shift: bool,
    /// Control key.
    pub control: bool,
    /// Alt/Option key.
    pub alt: bool,
    /// Platform command/super key.
    pub meta: bool,
}

/// Logical keys used by default controls. Platform-specific shortcuts can be
/// handled before this normalization; text comes separately from key identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable shortcut identity; may differ from produced text.
    Character(char),
    /// Return/Enter.
    Enter,
    /// Tab navigation, normally handled by the focus owner.
    Tab,
    /// Cancel current interaction.
    Escape,
    /// Delete previous grapheme.
    Backspace,
    /// Delete next grapheme.
    Delete,
    /// Previous visual cluster.
    Left,
    /// Next visual cluster.
    Right,
    /// Previous display line.
    Up,
    /// Next display line.
    Down,
    /// Start of line/document.
    Home,
    /// End of line/document.
    End,
    /// Move one page toward the start, or increase a range by a large step.
    PageUp,
    /// Move one page toward the end, or decrease a range by a large step.
    PageDown,
    /// Key without a default control action.
    Unidentified,
}

/// Borrowed keyboard input; routing need not allocate another text string.
#[derive(Clone, Copy, Debug)]
pub struct KeyInput<'a> {
    /// Logical key, independent of any committed text.
    pub key: Key,
    /// Translated text from the platform; empty for non-text keys.
    pub text: &'a str,
    /// Current modifiers.
    pub modifiers: Modifiers,
    /// Press versus release.
    pub pressed: bool,
    /// A held-key repeat, not a second initial activation.
    pub repeat: bool,
}

/// Primary-button pointer interactions used by default controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    /// Enter or move, including captured motion outside the control.
    Move,
    /// Primary-button press; host computes consecutive click count.
    Down {
        /// One, two or three for caret, word or line selection.
        clicks: u8,
    },
    /// Primary-button release.
    Up,
    /// Pointer left the control; capture may remain active.
    Leave,
    /// Device/capture loss: stop without activating or committing.
    Cancel,
}

/// Coordinates and containment use the same presented geometry as drawing.
#[derive(Clone, Copy, Debug)]
pub struct PointerInput {
    /// Originating pointer.
    pub id: PointerId,
    /// Pointer transition.
    pub kind: PointerKind,
    /// Position in the control's local logical coordinates.
    pub position: Point,
    /// Whether the point is within the control's effective hit region.
    pub inside: bool,
    /// Current keyboard modifiers.
    pub modifiers: Modifiers,
}

/// Platform-neutral input delivered after routing and default prevention.
#[derive(Clone, Copy, Debug)]
pub enum Input<'a> {
    /// Primary-pointer interaction.
    Pointer(PointerInput),
    /// Physical keyboard transition and translated text.
    Key(KeyInput<'a>),
    /// Logical control focus changed.
    Focus(bool),
    /// Cancel a press, capture or composition without activation.
    Cancel,
    /// Semantic activation from an application or assistive technology.
    Activate,
    /// Increase a range through the same behavior as the arrow keys.
    Increment,
    /// Decrease a range through the same behavior as the arrow keys.
    Decrement,
    /// Set a finite numeric value through the enabled range behavior.
    SetValue(f64),
    /// Replace an editor's selection with clipboard text as one undo group.
    Paste(&'a str),
    /// Native IME transaction; its serial/session routing remains platform-owned.
    #[cfg(feature = "text")]
    Ime(aegle_text::ImeEdit<'a>),
}

/// Semantic result independent of the input device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Button activation.
    Activate,
    /// Submit a single-line text field.
    Submit,
    /// A toggle or range value actually changed through user or semantic input.
    Change,
}

/// Clipboard transfer performed by the host's native clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clipboard {
    /// Store the editor's selected text.
    Copy,
    /// Store the selected text, then delete it by delivering `Input::Paste("")`.
    Cut,
    /// Read text and deliver it as [`Input::Paste`].
    Paste,
}

/// The host applies capture so a release reaches the original target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Route further events from this pointer to the target.
    Acquire(PointerId),
    /// Release only if this target still owns this pointer's capture.
    Release(PointerId),
}

/// Immediate host effects. A control owns behavior, never platform resources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// This control recognized the default action.
    pub handled: bool,
    /// Visual interaction state may have changed.
    pub repaint: bool,
    /// Focus or enabled state changed independently of pixels. Text value and
    /// selection invalidation also remain available through the editor.
    pub semantics: bool,
    /// Semantic action to dispatch through the host's ordinary action handler.
    pub action: Option<Action>,
    /// Ask the focus owner to focus this control.
    pub focus: bool,
    /// Change pointer capture.
    pub capture: Option<Capture>,
    /// Cancel the native IME session before publishing a fresh editor state.
    pub reset_ime: bool,
    /// Clipboard transfer to perform for an editor shortcut.
    pub clipboard: Option<Clipboard>,
}
