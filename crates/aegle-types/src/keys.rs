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
    /// The Menu (Application) key, which requests a context menu.
    ContextMenu,
    /// Function key F1 through F24.
    Function(u8),
    /// Key without a default control action.
    Unidentified,
}
