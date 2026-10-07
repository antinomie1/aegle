use crate::{FontStyle, FontWeight, FontWidth, Language, LineHeight};
use aegle_types::Color;
use std::{error::Error, fmt};

/// Styling for a display paragraph or plain text editor, in logical pixels.
///
/// Family names use CSS syntax, for example `"Noto Sans CJK SC", sans-serif`.
/// Paragraphs borrow this while shaping; editors retain owned styling so later
/// edits can use it without keeping this borrowed configuration alive.
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
    pub(crate) fn common_properties(&self) -> [parley::StyleProperty<'static, Color>; 9] {
        use parley::StyleProperty::*;
        [
            FontSize(self.size),
            FontWeight(self.weight),
            FontWidth(self.width),
            FontStyle(self.slant),
            Locale(self.locale),
            Brush(self.color),
            LineHeight(self.line_height),
            LetterSpacing(self.letter_spacing),
            WordSpacing(self.word_spacing),
        ]
    }

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

/// Invalid input to fonts, layout or retained editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextError {
    /// The blob did not contain any fonts that Fontique could register.
    InvalidFont,
    /// A numeric style value is non-finite or outside its documented range.
    InvalidStyle,
    /// A wrap width is non-finite or negative; use `None` for unconstrained text.
    InvalidWidth,
    /// Text exceeds the engine's 32-bit byte indexing range.
    TextTooLong,
    /// A UTF-8 byte range is reversed, out of bounds or splits a codepoint.
    InvalidRange,
    /// Hit-test coordinates or caret thickness are invalid.
    InvalidPosition,
    /// A user edit was requested while read-only.
    ReadOnly,
    /// A single-line editor was given a hard line separator.
    SingleLine,
    /// Finish or cancel preedit before ordinary selection/editing operations.
    CompositionActive,
    /// Text has no font-backed layout from which to derive a cursor.
    MissingFont,
    /// Password editors accept no input-method composition.
    Password,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidFont => "font data contains no registrable font",
            Self::InvalidStyle => "text style contains an invalid numeric value",
            Self::InvalidWidth => "text width must be finite and nonnegative",
            Self::TextTooLong => "text exceeds the 32-bit layout indexing limit",
            Self::InvalidRange => "text range must use valid UTF-8 boundaries",
            Self::InvalidPosition => "text coordinates or caret width are invalid",
            Self::ReadOnly => "text editor is read-only",
            Self::SingleLine => "single-line editor rejects line breaks",
            Self::CompositionActive => "finish or cancel preedit before this operation",
            Self::MissingFont => "text has no font-backed layout",
            Self::Password => "password editor rejects input-method composition",
        })
    }
}

impl Error for TextError {}
