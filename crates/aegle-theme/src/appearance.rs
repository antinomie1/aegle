use crate::{ControlKind, Theme};
use aegle_types::Color;
use core::fmt;

/// A snapshot of retained behavior used to compute a control's appearance.
///
/// The host supplies effective enabled state, including disabled ancestors.
/// Resolving this value never changes focus, input capture or editor state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualState {
    /// The control's kind.
    pub kind: &'static ControlKind,
    /// Whether the control can accept input.
    pub enabled: bool,
    /// Whether the pointer is over the control.
    pub hovered: bool,
    /// Whether its behavior is currently pressed.
    pub pressed: bool,
    /// Whether the control has keyboard focus.
    pub focused: bool,
    /// Whether an editor allows selection but disallows content changes.
    pub read_only: bool,
    /// Whether a checkbox, switch or radio button is checked; false for other
    /// roles and for a checkbox in the mixed state.
    pub checked: bool,
}

/// Resolved paint values, independent of layout and behavior.
///
/// Colors are unpremultiplied sRGB; widths and radius use logical pixels. The
/// host paints the focus outline separately, after the control's contents.
/// Zero outline width disables that outline. Transparent fills require no
/// drawing. Accept a custom skin result only after [`Self::validate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    /// Control fill.
    pub background: Color,
    /// Text and icon color.
    pub foreground: Color,
    /// Resting outline color, independent of focus.
    pub border_color: Color,
    /// Resting outline width; finite and nonnegative.
    pub border_width: f32,
    /// Corner radius; finite and nonnegative.
    pub radius: f32,
    /// Independent focus outline color.
    pub focus_color: Color,
    /// Focus outline width; finite and nonnegative. Zero when unfocused by default.
    pub focus_width: f32,
    /// Text selection fill, paired with the foreground color.
    pub selection: Color,
    /// Text caret and preedit indicator color.
    pub caret: Color,
    /// Check mark, selected switch thumb, range fill and slider thumb color.
    pub indicator: Color,
}

impl Appearance {
    /// The common starting point of skins: transparent and borderless, with
    /// the theme foreground (muted while disabled), the 2 dp accent focus
    /// outline while enabled and focused, the theme selection, an accent
    /// caret, and an accent indicator (muted while disabled).
    pub fn base(theme: &Theme, state: VisualState) -> Self {
        Self {
            background: Color::TRANSPARENT,
            foreground: if state.enabled {
                theme.foreground
            } else {
                theme.muted
            },
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: 0.0,
            focus_color: theme.accent,
            focus_width: if state.enabled && state.focused {
                2.0
            } else {
                0.0
            },
            selection: theme.selection,
            caret: theme.accent,
            indicator: if state.enabled {
                theme.accent
            } else {
                theme.muted
            },
        }
    }

    /// Rejects nonfinite or negative geometry from a custom skin.
    pub fn validate(&self) -> Result<(), InvalidStyle> {
        validate_geometry([
            ("border_width", Some(self.border_width)),
            ("radius", Some(self.radius)),
            ("focus_width", Some(self.focus_width)),
        ])
    }
}

/// A small stateless skin function that reuses the host's control behavior.
///
/// Compute appearance only from the supplied theme and visual state. The host
/// may call this during layout or painting while its tree is borrowed: a skin
/// must not reenter the UI, mutate controls or dispatch application callbacks.
/// Local [`Style`] overrides take precedence over the returned appearance.
pub type Skin = fn(&Theme, VisualState) -> Appearance;

/// Sparse local paint overrides, independent of font size and layout metrics.
///
/// `None` uses the skin's value. Hosts can store this only for styled nodes,
/// rather than enlarging every node. State backgrounds use the priority
/// disabled, pressed, hovered; focus stays an independent overlay. A selected
/// state with no override retains the base value, without falling through to a
/// lower-priority state. Values do not inherit along the control tree.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Style {
    /// Control fill override.
    pub background: Option<Color>,
    /// Text and icon color override.
    pub foreground: Option<Color>,
    /// Resting outline color override.
    pub border_color: Option<Color>,
    /// Resting outline width override; finite and nonnegative.
    pub border_width: Option<f32>,
    /// Corner radius override; finite and nonnegative.
    pub radius: Option<f32>,
    /// Independent focus outline color override.
    pub focus_color: Option<Color>,
    /// Focus outline width override; finite and nonnegative.
    pub focus_width: Option<f32>,
    /// Text selection fill override.
    pub selection: Option<Color>,
    /// Text caret and preedit indicator color override.
    pub caret: Option<Color>,
    /// Check mark, selected switch thumb, range fill and slider thumb override.
    pub indicator: Option<Color>,
    /// Fill used while enabled, hovered and not pressed.
    pub hover_background: Option<Color>,
    /// Fill used while enabled and pressed.
    pub pressed_background: Option<Color>,
    /// Fill used while disabled, regardless of hover or pressed state.
    pub disabled_background: Option<Color>,
    /// Text and icon color used while disabled.
    pub disabled_foreground: Option<Color>,
}

impl Style {
    /// Applies validated local values over a skin's resolved appearance.
    ///
    /// Base overrides apply first, followed by the highest-priority current
    /// state. This changes paint values only, never the supplied behavior state.
    pub fn apply(&self, appearance: &mut Appearance, state: VisualState) {
        for (target, value) in [
            (&mut appearance.background, self.background),
            (&mut appearance.foreground, self.foreground),
            (&mut appearance.border_color, self.border_color),
            (&mut appearance.focus_color, self.focus_color),
            (&mut appearance.selection, self.selection),
            (&mut appearance.caret, self.caret),
            (&mut appearance.indicator, self.indicator),
        ] {
            if let Some(value) = value {
                *target = value;
            }
        }
        for (target, value) in [
            (&mut appearance.border_width, self.border_width),
            (&mut appearance.radius, self.radius),
            (&mut appearance.focus_width, self.focus_width),
        ] {
            if let Some(value) = value {
                *target = value;
            }
        }
        let background = if !state.enabled {
            if let Some(foreground) = self.disabled_foreground {
                appearance.foreground = foreground;
            }
            self.disabled_background
        } else if state.pressed {
            self.pressed_background
        } else if state.hovered {
            self.hover_background
        } else {
            None
        };
        if let Some(background) = background {
            appearance.background = background;
        }
    }

    /// Validates local geometry before the host stores an override.
    pub fn validate(&self) -> Result<(), InvalidStyle> {
        validate_geometry([
            ("border_width", self.border_width),
            ("radius", self.radius),
            ("focus_width", self.focus_width),
        ])
    }
}

fn validate_geometry(values: [(&'static str, Option<f32>); 3]) -> Result<(), InvalidStyle> {
    for (field, value) in values {
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(InvalidStyle { field });
        }
    }
    Ok(())
}

/// Invalid geometry encountered when accepting a skin or local paint override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidStyle {
    /// Name of the field whose documented constraints were violated.
    pub field: &'static str,
}

impl fmt::Display for InvalidStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid style metric: {}", self.field)
    }
}

impl core::error::Error for InvalidStyle {}
