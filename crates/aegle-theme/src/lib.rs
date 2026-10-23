//! Typed, allocation-free theme values, independent of windows and controls.
//!
//! A host keeps one shared theme and resolves control appearance from it. Accept
//! a custom theme with [`Theme::validate`] before applying it. Replacing a theme
//! updates presentation; the host retains focus, text and IME state. Compare
//! metrics separately from colors to avoid layout work for a palette change.
//!
//! The supplied palettes are explicit choices. Observing operating-system
//! preferences and applying local overrides are responsibilities of the host.

#![no_std]

mod appearance;
mod kind;
mod token;

pub use appearance::{Appearance, InvalidStyle, Skin, Style, VisualState};
pub use kind::{Accepts, ControlKind};
pub use token::{BUILTIN_TOKENS, Font, Token, TokenDefault, TokenKind, TokenType, TokenValue};

use aegle_types::{Color, Point, Shadow};
use core::fmt;

/// The design tokens of the default controls: a restrained, flat visual
/// system in the spirit of Metro.
///
/// Rectangles carry commands and content (buttons, fields, panels, menus) and
/// stay square unless [`Self::radius`] rounds them; circles and pills mark
/// state and values (radio buttons, switches, slider thumbs) and stay round.
/// One accent marks what is chosen or current: checked boxes, the selected tab
/// underline, the chosen list entry's leading bar, focus and accent buttons.
/// Hover and press are translucent [`Self::hover`] and [`Self::pressed`] layers
/// painted over whatever fill a control has, so one pair serves every control.
///
/// Colors are unpremultiplied sRGB. Metrics use logical pixels; the host
/// applies device scale once during rendering. Fields are public so a theme
/// can be written as a struct literal, usually `Theme { accent, ..Theme::light() }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Window or root canvas fill.
    pub background: Color,
    /// Fill of fields, menus, popups, dialogs and tables, raised from the canvas.
    pub surface: Color,
    /// Resting fill of buttons.
    pub control: Color,
    /// Primary text and icons.
    pub foreground: Color,
    /// Secondary and disabled text.
    pub muted: Color,
    /// Outlines that identify an input (fields, check boxes, radio rings,
    /// switch tracks); at least 3:1 against the surface.
    pub border: Color,
    /// Quiet lines: separators, button and panel edges, range tracks.
    pub divider: Color,
    /// The one emphasis color: checked and selected marks, focus, accent buttons.
    pub accent: Color,
    /// Text and marks painted on an accent fill.
    pub on_accent: Color,
    /// Translucent layer over a hovered control's fill, or a menu entry's.
    pub hover: Color,
    /// Translucent layer over a pressed control's fill.
    pub pressed: Color,
    /// Elevation of floating surfaces: popups, menus, tooltips, dialogs and
    /// drawers. A transparent color draws none.
    pub shadow: Shadow,
    /// Default body text size; finite and strictly positive.
    pub font_size: f32,
    /// Default content padding; finite and nonnegative.
    pub padding: f32,
    /// Default space between sibling controls; finite and nonnegative.
    pub gap: f32,
    /// Corner radius of the rectangular controls and panels; finite and
    /// nonnegative. Zero, the default, keeps them square.
    pub radius: f32,
    /// Preferred control height; finite and strictly positive.
    pub control_height: f32,
}

impl Theme {
    /// The default light palette and desktop metrics.
    pub const fn light() -> Self {
        Self {
            background: Color::rgb(243, 243, 243),
            surface: Color::WHITE,
            control: Color::rgb(229, 229, 229),
            foreground: Color::rgb(26, 26, 26),
            muted: Color::rgb(97, 97, 97),
            border: Color::rgb(138, 138, 138),
            divider: Color::rgb(217, 217, 217),
            accent: Color::rgb(46, 107, 219),
            on_accent: Color::WHITE,
            hover: Color::rgba(0, 0, 0, 13),
            pressed: Color::rgba(0, 0, 0, 28),
            shadow: elevation(Color::rgba(0, 0, 0, 40)),
            font_size: 14.0,
            padding: 8.0,
            gap: 8.0,
            radius: 0.0,
            control_height: 32.0,
        }
    }

    /// A dark palette with the same metrics as [`Self::light`].
    pub const fn dark() -> Self {
        Self {
            background: Color::rgb(28, 28, 28),
            surface: Color::rgb(38, 38, 38),
            control: Color::rgb(54, 54, 54),
            foreground: Color::rgb(242, 242, 242),
            muted: Color::rgb(166, 166, 166),
            border: Color::rgb(140, 140, 140),
            divider: Color::rgb(64, 64, 64),
            accent: Color::rgb(53, 112, 224),
            on_accent: Color::WHITE,
            hover: Color::rgba(255, 255, 255, 15),
            pressed: Color::rgba(255, 255, 255, 30),
            shadow: elevation(Color::rgba(0, 0, 0, 140)),
            ..Self::light()
        }
    }

    /// A black high-contrast palette with white outlines and yellow marks.
    ///
    /// Every fill is black so outlines identify controls; hover and press
    /// layers are yellow. Disabled and secondary text use a distinct gray (8:1
    /// on black) rather than white, so disabled controls remain
    /// distinguishable. This is an explicit fallback palette, not a query of
    /// system colors.
    pub const fn high_contrast() -> Self {
        Self {
            background: Color::BLACK,
            surface: Color::BLACK,
            control: Color::BLACK,
            foreground: Color::WHITE,
            muted: Color::rgb(160, 160, 160),
            border: Color::WHITE,
            divider: Color::WHITE,
            accent: Color::rgb(255, 255, 0),
            on_accent: Color::BLACK,
            hover: Color::rgba(255, 255, 0, 72),
            pressed: Color::rgba(255, 255, 0, 112),
            shadow: elevation(Color::TRANSPARENT),
            ..Self::light()
        }
    }

    /// The text selection fill: the accent tinted into the surface, paired
    /// with the normal foreground.
    pub fn selection(&self) -> Color {
        self.surface.mix(self.accent, 0.3)
    }

    /// Validates custom metrics before a host accepts the theme.
    ///
    /// Returns the first invalid field. Colors have no invalid representation;
    /// custom palette contrast remains the application's responsibility.
    pub fn validate(&self) -> Result<(), InvalidTheme> {
        if !self.shadow.is_valid() {
            return Err(InvalidTheme { field: "shadow" });
        }
        for (field, value, positive) in [
            ("font_size", self.font_size, true),
            ("padding", self.padding, false),
            ("gap", self.gap, false),
            ("radius", self.radius, false),
            ("control_height", self.control_height, true),
        ] {
            if !value.is_finite() || value < 0.0 || (positive && value == 0.0) {
                return Err(InvalidTheme { field });
            }
        }
        Ok(())
    }
}

/// The soft drop shadow of the supplied palettes' floating surfaces.
const fn elevation(color: Color) -> Shadow {
    Shadow {
        offset: Point::new(0.0, 4.0),
        blur: 8.0,
        spread: 0.0,
        color,
    }
}

macro_rules! theme_override {
    ($($field:ident: $ty:ty,)*) => {
        /// Replaces chosen tokens of whatever theme it is applied to; unset
        /// tokens keep the base value, so an override follows later changes to
        /// its parent theme.
        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        pub struct ThemeOverride {
            $(
                #[doc = concat!("Replacement for [`Theme::", stringify!($field), "`].")]
                pub $field: Option<$ty>,
            )*
        }

        impl ThemeOverride {
            /// The base theme with every set token replaced. The result is not validated.
            pub fn apply(&self, base: &Theme) -> Theme {
                Theme {
                    $($field: self.$field.unwrap_or(base.$field),)*
                }
            }
        }
    };
}

theme_override! {
    background: Color,
    surface: Color,
    control: Color,
    foreground: Color,
    muted: Color,
    border: Color,
    divider: Color,
    accent: Color,
    on_accent: Color,
    hover: Color,
    pressed: Color,
    shadow: Shadow,
    font_size: f32,
    padding: f32,
    gap: f32,
    radius: f32,
    control_height: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

/// An invalid metric encountered while accepting a custom [`Theme`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTheme {
    /// Name of the field whose documented constraints were violated.
    pub field: &'static str,
}

impl fmt::Display for InvalidTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid theme metric: {}", self.field)
    }
}

impl core::error::Error for InvalidTheme {}
