use crate::{FontStyle, FontWeight, FontWidth, Language, LineHeight};
use aegle_types::Color;
use std::{error::Error, fmt};

/// Default styling for one display paragraph, in logical pixels.
///
/// Family names use CSS syntax, for example `"Noto Sans CJK SC", sans-serif`.
/// The style is borrowed only while shaping and is not stored in the paragraph.
#[derive(Clone, Debug)]
pub struct TextStyle<'a> {
    /// Ordered font family names or generic families.
    pub families: &'a str,
    /// Font size, finite and strictly positive.
    pub size: f32,
    /// Font weight, in the inclusive CSS range 1–1000.
    pub weight: FontWeight,
    /// Condensed or expanded face selection; ratio must be finite and positive.
    pub width: FontWidth,
    /// Upright, italic, or oblique face selection. Explicit oblique angles must
    /// be finite and strictly between -90 and 90 degrees.
    pub slant: FontStyle,
    /// Language hint for shaping and fallback, such as `zh-Hans` or `ja`.
    pub locale: Option<Language>,
    /// Unpremultiplied sRGB foreground color.
    pub color: Color,
    /// Positive line height, relative to metrics, size, or absolute pixels.
    pub line_height: LineHeight,
    /// Additional spacing between letters in logical pixels.
    pub letter_spacing: f32,
    /// Additional spacing between words in logical pixels.
    pub word_spacing: f32,
}

impl Default for TextStyle<'_> {
    fn default() -> Self {
        Self {
            families: "sans-serif",
            size: 16.0,
            weight: FontWeight::NORMAL,
            width: FontWidth::NORMAL,
            slant: FontStyle::Normal,
            locale: None,
            color: Color::BLACK,
            line_height: LineHeight::default(),
            letter_spacing: 0.0,
            word_spacing: 0.0,
        }
    }
}

impl TextStyle<'_> {
    pub(crate) fn validate(&self) -> Result<(), TextError> {
        let line_height = match self.line_height {
            LineHeight::MetricsRelative(value)
            | LineHeight::FontSizeRelative(value)
            | LineHeight::Absolute(value) => value,
        };
        let valid_slant = match self.slant {
            FontStyle::Oblique(Some(angle)) => angle.is_finite() && angle.abs() < 90.0,
            _ => true,
        };
        if ![self.size, self.width.ratio(), line_height]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            || !(1.0..=1000.0).contains(&self.weight.value())
            || !self.letter_spacing.is_finite()
            || !self.word_spacing.is_finite()
            || !valid_slant
        {
            return Err(TextError::InvalidStyle);
        }
        Ok(())
    }
}

/// Invalid input to font registration or paragraph layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    /// The blob did not contain any fonts that Fontique could register.
    InvalidFont,
    /// A numeric style value is non-finite or outside its documented range.
    InvalidStyle,
    /// A wrap width is non-finite or negative; use `None` for unconstrained text.
    InvalidWidth,
    /// Text exceeds the engine's 32-bit byte indexing range.
    TextTooLong,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidFont => "font data contains no registrable font",
            Self::InvalidStyle => "text style contains an invalid numeric value",
            Self::InvalidWidth => "text width must be finite and nonnegative",
            Self::TextTooLong => "text exceeds the 32-bit layout indexing limit",
        })
    }
}

impl Error for TextError {}
