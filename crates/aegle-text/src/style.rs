use crate::{LineHeight, Slant};
use aegle_types::Color;
#[cfg(any(
    feature = "text-parley",
    not(any(target_os = "linux", target_os = "windows", target_os = "macos"))
))]
use parley::fontique::Language;
use std::{error::Error, fmt, sync::Arc};

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
    /// Font weight, in the inclusive CSS range 1–1000; 400 is normal, 700 bold.
    pub weight: f32,
    /// Condensed or expanded face selection as a finite positive ratio; 1 is normal.
    pub width: f32,
    /// Upright, italic, or oblique face selection.
    pub slant: Slant,
    /// BCP 47 language hint for shaping and fallback, such as `zh-Hans` or `ja`.
    pub locale: Option<&'a str>,
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
            weight: 400.0,
            width: 1.0,
            slant: Slant::Normal,
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
        let valid_slant = match self.slant {
            Slant::Oblique(Some(angle)) => angle.is_finite() && angle.abs() < 90.0,
            _ => true,
        };
        if ![self.size, self.width, self.line_height.value()]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            || !(1.0..=1000.0).contains(&self.weight)
            || !self.letter_spacing.is_finite()
            || !self.word_spacing.is_finite()
            || !valid_slant
            || self.locale.is_some_and(|locale| !is_valid_bcp47(locale))
        {
            return Err(TextError::InvalidStyle);
        }
        Ok(())
    }

    /// The parsed language hint of a validated style.
    pub(crate) fn language(&self) -> Option<String> {
        self.locale.map(|l| l.to_string())
    }
}

fn is_valid_bcp47(locale: &str) -> bool {
    !locale.is_empty()
        && locale.split('-').all(|part| {
            !part.is_empty() && part.len() <= 8 && part.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

/// What shaping depends on: every [`TextStyle`] field except the color, with
/// families interned by the [`crate::TextSystem`]. Retained text keeps it to
/// reshape itself and to tell a color change, which needs no shaping, from
/// the rest.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shaping {
    pub families: Arc<str>,
    pub size: f32,
    pub weight: f32,
    pub width: f32,
    pub slant: Slant,
    pub locale: Option<String>,
    pub line_height: LineHeight,
    pub letter_spacing: f32,
    pub word_spacing: f32,
}

impl Shaping {
    /// Whether `style` shapes as this does.
    pub fn matches(&self, style: &TextStyle<'_>) -> bool {
        *self.families == *style.families
            && self.size == style.size
            && self.weight == style.weight
            && self.width == style.width
            && self.slant == style.slant
            && self.locale == style.language()
            && self.line_height == style.line_height
            && self.letter_spacing == style.letter_spacing
            && self.word_spacing == style.word_spacing
    }

    #[cfg(any(
        feature = "text-parley",
        not(any(target_os = "linux", target_os = "windows", target_os = "macos"))
    ))]
    /// Builder defaults, with `color` as every glyph's brush.
    pub fn properties(&self, color: Color) -> [parley::StyleProperty<'_, Color>; 10] {
        use parley::StyleProperty::*;
        [
            FontFamily(parley::FontFamily::Source((&*self.families).into())),
            FontSize(self.size),
            FontWeight(parley::FontWeight::new(self.weight)),
            FontWidth(parley::FontWidth::from_ratio(self.width)),
            FontStyle(self.slant.parley()),
            Locale(self.locale.as_deref().and_then(|l| Language::parse(l).ok())),
            Brush(color),
            LineHeight(self.line_height.parley()),
            LetterSpacing(self.letter_spacing),
            WordSpacing(self.word_spacing),
        ]
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
    /// The paragraph's layout was released; restore it with
    /// [`crate::TextSystem::restore`] first.
    Released,
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
            Self::Released => "paragraph layout was released; restore it first",
        })
    }
}

impl Error for TextError {}
