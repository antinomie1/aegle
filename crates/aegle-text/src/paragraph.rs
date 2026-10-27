use crate::{Alignment, ContentWidths, TextError, layout::Layout, style::Shaping};
use aegle_types::{Color, Size};

/// Visible missing-glyph and omitted-text diagnostics for a shaped paragraph.
///
/// Missing glyphs use the chosen font's `.notdef` glyph. If no font can be
/// selected, there is no glyph to draw: those bytes are reported separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextDiagnostics {
    /// Number of glyph IDs equal to zero, excluding control-only clusters.
    pub missing_glyphs: usize,
    /// UTF-8 bytes for which no font produced a layout run.
    pub unshaped_bytes: usize,
}

/// A retained display paragraph with owned UTF-8 text and shared font resources.
///
/// Reflow does not query fonts or shape text again. Replace text and styles with
/// [`crate::TextSystem::update`] or [`crate::TextSystem::restyle`]; a new color
/// alone needs no shaping. Font changes in the collection affect this
/// paragraph only after such an explicit rebuild.
///
/// A paragraph nobody shows can [`release`](Self::release) its layout and be
/// shaped again by [`crate::TextSystem::restore`] when it is needed.
#[derive(Debug)]
pub struct Paragraph {
    pub(crate) text: String,
    pub(crate) layout: Layout,
    pub(crate) width: Option<f32>,
    pub(crate) alignment: Alignment,
    pub(crate) diagnostics: TextDiagnostics,
    pub(crate) content_widths: ContentWidths,
    /// What the layout was shaped with; its weight decides synthetic bold.
    pub(crate) shaping: Shaping,
    /// Foreground color, applied when painting so that changing it needs no
    /// shaping.
    pub(crate) color: Color,
    /// [`Self::release`] emptied the layout.
    pub(crate) released: bool,
}

impl Paragraph {
    /// Returns the original UTF-8 text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The foreground color painting uses.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Frees the layout, keeping text, style, width, alignment, content widths
    /// and diagnostics; [`crate::TextSystem::restore`] shapes it again. Layout
    /// queries see an empty layout meanwhile, reflowing returns
    /// [`TextError::Released`] and painting panics. For text no one shows.
    pub fn release(&mut self) {
        self.layout = Layout::empty();
        self.released = true;
    }

    /// Whether [`Self::release`] emptied the layout.
    pub fn is_released(&self) -> bool {
        self.released
    }

    /// Changes wrapping and alignment, returning the new logical size.
    ///
    /// `None` disables soft wrapping. A finite width of zero is valid for minimum
    /// content measurement. Identical constraints reuse the current line layout.
    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) -> Result<Size, TextError> {
        if width.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(TextError::InvalidWidth);
        }
        if self.released {
            return Err(TextError::Released);
        }
        if self.width != width || self.alignment != alignment {
            self.width = width;
            self.alignment = alignment;
            self.layout.reflow(width, alignment);
        }
        Ok(self.size())
    }

    /// Returns the current line-layout dimensions, excluding trailing whitespace.
    pub fn size(&self) -> Size {
        Size::new(self.layout.width(), self.layout.height())
    }

    /// Number of laid out lines; zero while released.
    pub fn line_count(&self) -> usize {
        self.layout.len()
    }

    /// Offset of the first line's baseline from the top, if there is a line.
    pub fn first_baseline(&self) -> Option<f32> {
        self.layout.first_baseline()
    }

    /// Returns intrinsic soft-wrap limits from the shaping result.
    ///
    /// Parley currently documents imperfect estimates for mixed-direction text.
    pub fn content_widths(&self) -> ContentWidths {
        ContentWidths {
            min: self.content_widths.min,
            max: self.content_widths.max,
        }
    }

    /// Reports missing glyphs and text that could not be assigned any font.
    ///
    /// This is an explicit font-policy boundary: a host may reject incomplete
    /// paragraphs or show its own missing-font indicator. There is no bundled
    /// universal fallback font and no guarantee of coverage from system fonts.
    pub fn diagnostics(&self) -> TextDiagnostics {
        self.diagnostics
    }

    /// Returns the missing glyph count; also inspect [`Self::diagnostics`] when
    /// no matching font may be available.
    pub fn missing_glyphs(&self) -> usize {
        self.diagnostics.missing_glyphs
    }

    pub(crate) fn update_diagnostics(&mut self) {
        self.diagnostics = self.layout.diagnostics(&self.text);
    }
}
