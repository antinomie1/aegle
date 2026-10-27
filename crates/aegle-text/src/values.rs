//! Aegle's own text vocabulary; the shaping engine's types stay inside.

/// Horizontal placement of lines within the layout width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    /// The start edge of each line's own direction.
    #[default]
    Start,
    /// The end edge of each line's own direction.
    End,
    /// The left edge.
    Left,
    /// Centered.
    Center,
    /// The right edge.
    Right,
    /// Stretched to both edges, except the last line.
    Justify,
}

impl Alignment {
    #[cfg(any(
        feature = "text-parley",
        not(any(target_os = "linux", target_os = "windows"))
    ))]
    pub(crate) fn parley(self) -> parley::Alignment {
        match self {
            Self::Start => parley::Alignment::Start,
            Self::End => parley::Alignment::End,
            Self::Left => parley::Alignment::Left,
            Self::Center => parley::Alignment::Center,
            Self::Right => parley::Alignment::Right,
            Self::Justify => parley::Alignment::Justify,
        }
    }
}

/// The narrowest width a paragraph can wrap to and its unwrapped width.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentWidths {
    /// The longest unbreakable run, such as a word.
    pub min: f32,
    /// The width without soft wrapping.
    pub max: f32,
}

/// The height of a line box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// A multiple of the font's own line metrics, the default (1.0).
    MetricsRelative(f32),
    /// A multiple of the font size.
    FontSizeRelative(f32),
    /// Logical pixels.
    Absolute(f32),
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::MetricsRelative(1.0)
    }
}

impl LineHeight {
    pub(crate) fn value(self) -> f32 {
        match self {
            Self::MetricsRelative(value)
            | Self::FontSizeRelative(value)
            | Self::Absolute(value) => value,
        }
    }

    #[cfg(any(
        feature = "text-parley",
        not(any(target_os = "linux", target_os = "windows"))
    ))]
    pub(crate) fn parley(self) -> parley::LineHeight {
        match self {
            Self::MetricsRelative(value) => parley::LineHeight::MetricsRelative(value),
            Self::FontSizeRelative(value) => parley::LineHeight::FontSizeRelative(value),
            Self::Absolute(value) => parley::LineHeight::Absolute(value),
        }
    }
}

/// Upright, italic or oblique face selection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Slant {
    /// Upright.
    #[default]
    Normal,
    /// The italic face.
    Italic,
    /// An oblique face, at this angle in degrees strictly between -90 and
    /// 90 when given.
    Oblique(Option<f32>),
}

impl Slant {
    #[cfg(any(
        feature = "text-parley",
        not(any(target_os = "linux", target_os = "windows"))
    ))]
    pub(crate) fn parley(self) -> parley::FontStyle {
        match self {
            Self::Normal => parley::FontStyle::Normal,
            Self::Italic => parley::FontStyle::Italic,
            Self::Oblique(angle) => parley::FontStyle::Oblique(angle),
        }
    }
}

/// A CSS generic family that application fonts can stand in for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GenericFamily {
    /// `serif`.
    Serif,
    /// `sans-serif`.
    SansSerif,
    /// `monospace`.
    Monospace,
    /// `cursive`.
    Cursive,
    /// `fantasy`.
    Fantasy,
    /// `system-ui`, the platform's interface font.
    SystemUi,
    /// `emoji`.
    Emoji,
}

impl GenericFamily {
    #[cfg(any(
        feature = "text-parley",
        not(any(target_os = "linux", target_os = "windows"))
    ))]
    pub(crate) fn fontique(self) -> parley::fontique::GenericFamily {
        use parley::fontique::GenericFamily as Generic;
        match self {
            Self::Serif => Generic::Serif,
            Self::SansSerif => Generic::SansSerif,
            Self::Monospace => Generic::Monospace,
            Self::Cursive => Generic::Cursive,
            Self::Fantasy => Generic::Fantasy,
            Self::SystemUi => Generic::SystemUi,
            Self::Emoji => Generic::Emoji,
        }
    }
}

#[cfg(any(
    feature = "text-parley",
    not(any(target_os = "linux", target_os = "windows"))
))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontFamily(pub(crate) parley::fontique::FamilyId);

#[cfg(not(any(
    feature = "text-parley",
    not(any(target_os = "linux", target_os = "windows"))
)))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontFamily(pub(crate) u32);
