use crate::Skin;
use core::fmt;

/// A kind of control: its default skin and the local style it accepts.
///
/// Every kind is a `static` that a control library declares once, the
/// built-in ones included; controls return a reference to it, and kinds
/// compare by address. The skin is the kind's default look, which inherited
/// and per-node skins replace.
///
/// ```
/// use aegle_theme::{Accepts, Appearance, ControlKind};
/// pub static CHIP: ControlKind = ControlKind {
///     name: "Chip",
///     skin: |theme, state| Appearance {
///         background: theme.surface,
///         ..Appearance::base(theme, state)
///     },
///     accepts: Accepts::TEXT.with(Accepts::INTERACTIVE),
///     container: false,
/// };
/// ```
pub struct ControlKind {
    /// Name for diagnostics, conventionally the control's type.
    pub name: &'static str,
    /// The default skin.
    pub skin: Skin,
    /// The kind-specific local style groups it accepts.
    pub accepts: Accepts,
    /// Lays out children: padding insets them rather than its own content.
    pub container: bool,
}

impl PartialEq for ControlKind {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for ControlKind {}

impl fmt::Debug for ControlKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// Groups of kind-specific local style, beyond the fields every control
/// accepts (background, foreground, border, radius and disabled colors).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accepts(u8);

impl Accepts {
    /// No kind-specific style.
    pub const NONE: Self = Self(0);
    /// Font size and face of a text-bearing control.
    pub const TEXT: Self = Self(1);
    /// Hover background and focus outline color and width.
    pub const INTERACTIVE: Self = Self(2);
    /// Pressed background.
    pub const PRESSED: Self = Self(4);
    /// Check mark, thumb and fill color.
    pub const INDICATOR: Self = Self(8);
    /// Selection and caret colors; the control has an editor.
    pub const EDITOR: Self = Self(16);

    /// Both groups.
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Whether every group of `other` is accepted.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}
