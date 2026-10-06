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
mod token;

pub use appearance::{Appearance, ControlKind, InvalidStyle, Skin, Style, VisualState};
pub use token::{BUILTIN_TOKENS, Font, Token, TokenDefault, TokenKind, TokenType, TokenValue};

use aegle_types::Color;
use core::fmt;

/// A compact theme for neutral, minimal controls.
///
/// The supplied palettes use square corners; a positive [`Self::radius`] rounds
/// every default control consistently. Colors are unpremultiplied sRGB. Metrics use logical pixels; the host applies
/// device scale once during rendering. Fields are public so a component library
/// can construct a theme without a builder or string-based token registry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Window or root canvas fill.
    pub background: Color,
    /// Resting control and panel fill.
    pub surface: Color,
    /// Primary text and icons on the canvas or surface.
    pub foreground: Color,
    /// Secondary text and icons.
    pub muted: Color,
    /// Focus ring and accent marks; not a text-bearing control fill.
    pub accent: Color,
    /// Control outlines and separators.
    pub border: Color,
    /// Opaque fill for a hovered control.
    pub hover: Color,
    /// Opaque fill for a pressed control.
    pub pressed: Color,
    /// Text selection fill, paired with the normal foreground color.
    pub selection: Color,
    /// Default body text size; finite and strictly positive.
    pub font_size: f32,
    /// Default content padding; finite and nonnegative.
    pub padding: f32,
    /// Default space between sibling controls; finite and nonnegative.
    pub gap: f32,
    /// Corner radius shared by default controls; finite and nonnegative.
    /// Zero, the default, gives square corners.
    pub radius: f32,
    /// Preferred control height; finite and strictly positive.
    pub control_height: f32,
}

impl Theme {
    /// The default light palette and desktop metrics.
    pub const fn light() -> Self {
        Self {
            background: Color::rgb(246, 247, 249),
            surface: Color::WHITE,
            foreground: Color::rgb(32, 36, 43),
            muted: Color::rgb(91, 100, 114),
            accent: Color::rgb(53, 92, 218),
            border: Color::rgb(115, 125, 140),
            hover: Color::rgb(236, 239, 245),
            pressed: Color::rgb(222, 229, 240),
            selection: Color::rgb(213, 223, 255),
            font_size: 14.0,
            padding: 8.0,
            gap: 8.0,
            radius: 0.0,
            control_height: 36.0,
        }
    }

    /// A dark palette with the same metrics as [`Self::light`].
    pub const fn dark() -> Self {
        Self {
            background: Color::rgb(21, 24, 29),
            surface: Color::rgb(30, 35, 43),
            foreground: Color::rgb(240, 242, 245),
            muted: Color::rgb(173, 182, 196),
            accent: Color::rgb(158, 181, 255),
            border: Color::rgb(120, 133, 150),
            hover: Color::rgb(42, 49, 60),
            pressed: Color::rgb(47, 55, 68),
            selection: Color::rgb(45, 64, 118),
            ..Self::light()
        }
    }

    /// A dark high-contrast palette with white outlines and yellow focus marks.
    ///
    /// Disabled and secondary text use a distinct gray (8:1 on black) rather
    /// than white, so disabled controls remain distinguishable from enabled
    /// ones. This is an explicit fallback palette, not a query of system colors.
    pub const fn high_contrast() -> Self {
        Self {
            background: Color::BLACK,
            surface: Color::BLACK,
            foreground: Color::WHITE,
            muted: Color::rgb(160, 160, 160),
            accent: Color::rgb(255, 255, 0),
            border: Color::WHITE,
            hover: Color::rgb(34, 34, 34),
            pressed: Color::rgb(64, 64, 64),
            selection: Color::rgb(28, 52, 134),
            ..Self::light()
        }
    }

    /// Validates custom metrics before a host accepts the theme.
    ///
    /// Returns the first invalid field. Colors have no invalid representation;
    /// custom palette contrast remains the application's responsibility.
    pub fn validate(&self) -> Result<(), InvalidTheme> {
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

/// Replaces chosen tokens of whatever theme it is applied to; unset tokens keep
/// the base value, so an override follows later changes to its parent theme.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThemeOverride {
    /// Replacement for [`Theme::background`].
    pub background: Option<Color>,
    /// Replacement for [`Theme::surface`].
    pub surface: Option<Color>,
    /// Replacement for [`Theme::foreground`].
    pub foreground: Option<Color>,
    /// Replacement for [`Theme::muted`].
    pub muted: Option<Color>,
    /// Replacement for [`Theme::accent`].
    pub accent: Option<Color>,
    /// Replacement for [`Theme::border`].
    pub border: Option<Color>,
    /// Replacement for [`Theme::hover`].
    pub hover: Option<Color>,
    /// Replacement for [`Theme::pressed`].
    pub pressed: Option<Color>,
    /// Replacement for [`Theme::selection`].
    pub selection: Option<Color>,
    /// Replacement for [`Theme::font_size`].
    pub font_size: Option<f32>,
    /// Replacement for [`Theme::padding`].
    pub padding: Option<f32>,
    /// Replacement for [`Theme::gap`].
    pub gap: Option<f32>,
    /// Replacement for [`Theme::radius`].
    pub radius: Option<f32>,
    /// Replacement for [`Theme::control_height`].
    pub control_height: Option<f32>,
}

impl ThemeOverride {
    /// The base theme with every set token replaced. The result is not validated.
    pub fn apply(&self, base: &Theme) -> Theme {
        Theme {
            background: self.background.unwrap_or(base.background),
            surface: self.surface.unwrap_or(base.surface),
            foreground: self.foreground.unwrap_or(base.foreground),
            muted: self.muted.unwrap_or(base.muted),
            accent: self.accent.unwrap_or(base.accent),
            border: self.border.unwrap_or(base.border),
            hover: self.hover.unwrap_or(base.hover),
            pressed: self.pressed.unwrap_or(base.pressed),
            selection: self.selection.unwrap_or(base.selection),
            font_size: self.font_size.unwrap_or(base.font_size),
            padding: self.padding.unwrap_or(base.padding),
            gap: self.gap.unwrap_or(base.gap),
            radius: self.radius.unwrap_or(base.radius),
            control_height: self.control_height.unwrap_or(base.control_height),
        }
    }
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
