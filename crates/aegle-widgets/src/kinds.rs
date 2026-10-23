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
    /// A push button: a flat block of the control fill.
    BUTTON = "Button", button, TEXT.with(PRESSABLE), false;
    /// The emphasized button of a group: a block of the accent fill.
    ACCENT_BUTTON = "AccentButton", accent_button, TEXT.with(PRESSABLE), false;
    /// A dropdown's face: a field-like box with a chevron.
    DROPDOWN = "Dropdown", dropdown, TEXT.with(PRESSABLE), false;
    /// A tab of a tab list: muted text, the selected one underlined.
    TAB = "Tab", tab, TEXT.with(PRESSABLE), false;
    /// A menu entry or a dropdown choice: borderless, layered while hovered,
    /// pressed or keyboard focused.
    MENU_ITEM = "MenuItem", menu_item, TEXT.with(PRESSABLE), false;
    /// A single-line or multiline text editor.
    TEXT_FIELD = "TextField", text_field, TEXT.with(Accepts::INTERACTIVE).with(Accepts::EDITOR), false;
    /// A labeled check box; background and border are the box's.
    CHECK_BOX = "CheckBox", toggle, TEXT.with(PRESSABLE).with(Accepts::INDICATOR), false;
    /// A labeled switch; background and border are the track's.
    SWITCH = "Switch", toggle, TEXT.with(PRESSABLE).with(Accepts::INDICATOR), false;
    /// A labeled radio button; background and border are the ring's.
    RADIO_BUTTON = "RadioButton", toggle, TEXT.with(PRESSABLE).with(Accepts::INDICATOR), false;
    /// A slider; the background is its track, the border its thumb's ring.
    SLIDER = "Slider", range, PRESSABLE.with(Accepts::INDICATOR), false;
    /// A progress bar; the background is its track.
    PROGRESS = "Progress", range, Accepts::INDICATOR, false;
    /// A scrollable container with a quiet edge.
    SCROLL_VIEW = "ScrollView", scroll_view, Accepts::NONE, true;
    /// The surface of popups, menus, tables, dialogs and drawers.
    PANEL = "Panel", panel, Accepts::NONE, true;
    /// A menu bar: a strip of the surface its entries sit on.
    MENU_BAR = "MenuBar", menu_bar, Accepts::NONE, true;
    /// A tooltip's text on a small raised surface.
    TOOLTIP = "Tooltip", panel, TEXT, false;
    /// A tree view container: a surface for hierarchical tree nodes.
    TREE_VIEW = "TreeView", scroll_view, Accepts::NONE, true;
    /// A single selectable tree row with hover and press layers; the
    /// selected row has a leading accent bar.
    TREE_NODE = "TreeNode", menu_item, TEXT.with(PRESSABLE), true;
    /// An in-window modal dialog card.
    DIALOG = "Dialog", panel, Accepts::NONE, true;
    /// An edge-docked navigation drawer panel.
    DRAWER = "Drawer", panel, Accepts::NONE, true;
}

/// `rest` under the theme's pressed or hover layer while enabled and pressed
/// or hovered.
fn pressable(theme: &Theme, state: VisualState, rest: Color) -> Color {
    match state.enabled {
        true if state.pressed => theme.pressed.over(rest),
        true if state.hovered => theme.hover.over(rest),
        _ => rest,
    }
}

/// A rectangle filled with `background` and edged with `border`.
fn framed(theme: &Theme, state: VisualState, background: Color, border: Color) -> Appearance {
    Appearance {
        background,
        border_color: border,
        border_width: 1.0,
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

/// The input outline: the border, darkening to muted while hovered.
fn input_border(theme: &Theme, state: VisualState) -> Color {
    match state.enabled {
        true if state.hovered || state.pressed => theme.muted,
        true => theme.border,
        false => theme.divider,
    }
}

/// A flat block of the control fill with a quiet edge, layered while
/// hovered or pressed.
pub fn button(theme: &Theme, state: VisualState) -> Appearance {
    framed(
        theme,
        state,
        pressable(theme, state, theme.control),
        theme.divider,
    )
}

/// A block of the accent fill with on-accent text; its focus outline uses the
/// foreground, which the accent fill would hide. Disabled, it reads as a
/// disabled [`button`].
pub fn accent_button(theme: &Theme, state: VisualState) -> Appearance {
    if !state.enabled {
        return button(theme, state);
    }
    let background = pressable(theme, state, theme.accent);
    Appearance {
        foreground: theme.on_accent,
        focus_color: theme.foreground,
        ..framed(theme, state, background, background)
    }
}

fn dropdown(theme: &Theme, state: VisualState) -> Appearance {
    let rest = if state.enabled {
        theme.surface
    } else {
        Color::TRANSPARENT
    };
    framed(
        theme,
        state,
        pressable(theme, state, rest),
        input_border(theme, state),
    )
}

/// A tab is checked while selected: selected or hovered tabs read in the
/// foreground, others muted.
fn tab(theme: &Theme, state: VisualState) -> Appearance {
    let base = Appearance::base(theme, state);
    Appearance {
        foreground: match state.enabled && (state.checked || state.hovered || state.pressed) {
            true => theme.foreground,
            false => theme.muted,
        },
        radius: theme.radius,
        ..base
    }
}

/// Menu entries show keyboard focus as the hover layer rather than an outline.
fn menu_item(theme: &Theme, state: VisualState) -> Appearance {
    let hovered = state.hovered || state.focused;
    Appearance {
        background: pressable(theme, VisualState { hovered, ..state }, Color::TRANSPARENT),
        radius: theme.radius,
        focus_width: 0.0,
        ..Appearance::base(theme, state)
    }
}

fn text_field(theme: &Theme, state: VisualState) -> Appearance {
    let background = if state.enabled && !state.read_only {
        theme.surface
    } else {
        Color::TRANSPARENT
    };
    let border = if state.read_only && state.enabled {
        theme.divider
    } else {
        input_border(theme, state)
    };
    framed(theme, state, background, border)
}

/// The marker of a check box, radio button or switch: an outlined surface
/// while off, an accent fill while checked.
fn toggle(theme: &Theme, state: VisualState) -> Appearance {
    let base = Appearance::base(theme, state);
    let (background, border) = match (state.enabled, state.checked) {
        (true, true) => {
            let fill = pressable(theme, state, theme.accent);
            (fill, fill)
        }
        (true, false) => (
            pressable(theme, state, theme.surface),
            input_border(theme, state),
        ),
        (false, true) => (base.indicator, base.indicator),
        (false, false) => (Color::TRANSPARENT, theme.divider),
    };
    Appearance {
        background,
        border_color: border,
        border_width: 1.0,
        radius: theme.radius,
        ..base
    }
}

/// A divider track, an accent fill and, on a slider, a surface thumb with a
/// border ring.
fn range(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.divider,
        border_color: theme.border,
        on_indicator: theme.surface,
        ..Appearance::base(theme, state)
    }
}

fn scroll_view(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        border_color: theme.divider,
        border_width: 1.0,
        radius: theme.radius,
        ..Appearance::base(theme, state)
    }
}

fn panel(theme: &Theme, state: VisualState) -> Appearance {
    framed(theme, state, theme.surface, theme.divider)
}

fn menu_bar(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.surface,
        ..Appearance::base(theme, state)
    }
}
