//! The kinds of the default controls and their neutral skins.
//!
//! Each kind is a plain [`ControlKind`] static, declared the way any control
//! library declares its own. Its skin is the neutral default look: replace it
//! for a whole subtree with `Node::set_kind_skin` or for one control with
//! `Node::set_skin`. Layout-only containers, canvases, images and separators
//! use the engine's [`aegle_ui::CONTAINER`].

use aegle_theme::{Accepts, Appearance, ControlKind, Theme, VisualState};
use aegle_types::Color;

const TEXT: Accepts = Accepts::TEXT;
const PRESSABLE: Accepts = Accepts::INTERACTIVE.with(Accepts::PRESSED);

macro_rules! kinds {
    ($($(#[$doc:meta])* $name:ident = $display:literal, $skin:path, $accepts:expr, $container:literal;)*) => {$(
        $(#[$doc])*
        pub static $name: ControlKind = ControlKind {
            name: $display,
            skin: $skin,
            accepts: $accepts,
            container: $container,
        };
    )*};
}

kinds! {
    /// Display text.
    LABEL = "Label", Appearance::base, TEXT, false;
    /// A push button, also a dropdown's face.
    BUTTON = "Button", button, TEXT.with(PRESSABLE), false;
    /// A tab of a tab list: no box, a tint while hovered or pressed.
    TAB = "Tab", tab, TEXT.with(PRESSABLE), false;
    /// A menu entry or a dropdown choice: a button without a border.
    MENU_ITEM = "MenuItem", menu_item, TEXT.with(PRESSABLE), false;
    /// A single-line or multiline text editor.
    TEXT_FIELD = "TextField", text_field, TEXT.with(Accepts::INTERACTIVE).with(Accepts::EDITOR), false;
    /// A labeled check box.
    CHECK_BOX = "CheckBox", toggle, TEXT.with(PRESSABLE).with(Accepts::INDICATOR), false;
    /// A labeled switch.
    SWITCH = "Switch", toggle, TEXT.with(PRESSABLE).with(Accepts::INDICATOR), false;
    /// A labeled radio button.
    RADIO_BUTTON = "RadioButton", toggle, TEXT.with(PRESSABLE).with(Accepts::INDICATOR), false;
    /// A slider; the background is its track.
    SLIDER = "Slider", range, PRESSABLE.with(Accepts::INDICATOR), false;
    /// A progress bar; the background is its track.
    PROGRESS = "Progress", range, Accepts::INDICATOR, false;
    /// A scrollable container with a border.
    SCROLL_VIEW = "ScrollView", scroll_view, Accepts::NONE, true;
    /// The bordered surface of popups, menus and tables.
    PANEL = "Panel", panel, Accepts::NONE, true;
    /// A menu bar: a strip of the surface its entries sit on.
    MENU_BAR = "MenuBar", menu_bar, Accepts::NONE, true;
    /// A tooltip's text, in inverted colors.
    TOOLTIP = "Tooltip", tooltip, TEXT, false;
}

/// The pressed or hovered fill of an enabled control, else `rest`.
fn pressable(theme: &Theme, state: VisualState, rest: Color) -> Color {
    match state.enabled {
        true if state.pressed => theme.pressed,
        true if state.hovered => theme.hover,
        _ => rest,
    }
}

/// A rounded, bordered box with `background`.
fn framed(theme: &Theme, state: VisualState, background: Color) -> Appearance {
    Appearance {
        background,
        border_color: theme.border,
        border_width: 1.0,
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

/// The neutral button: the surface, tinted while hovered or pressed.
pub fn button(theme: &Theme, state: VisualState) -> Appearance {
    framed(theme, state, pressable(theme, state, theme.surface))
}

fn tab(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: pressable(theme, state, Color::TRANSPARENT),
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

fn menu_item(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: pressable(theme, state, theme.surface),
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

fn text_field(theme: &Theme, state: VisualState) -> Appearance {
    framed(theme, state, theme.surface)
}

fn toggle(theme: &Theme, state: VisualState) -> Appearance {
    framed(theme, state, pressable(theme, state, Color::TRANSPARENT))
}

fn range(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.border,
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

fn scroll_view(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        border_color: theme.border,
        border_width: 1.0,
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

fn panel(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.surface,
        border_color: theme.border,
        border_width: 1.0,
        ..Appearance::base(theme, state)
    }
}

fn menu_bar(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.surface,
        ..Appearance::base(theme, state)
    }
}

fn tooltip(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.foreground,
        foreground: theme.background,
        ..Appearance::base(theme, state)
    }
}
