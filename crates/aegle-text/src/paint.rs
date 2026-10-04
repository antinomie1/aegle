use crate::{Layout, Paragraph, TextDiagnostics};
use aegle_scene::{Glyph, GlyphRun, SceneBuilder, SceneError};
use aegle_types::{Color, Point};
use parley::PositionedLayoutItem;
use std::{error::Error, fmt};

impl Paragraph {
    /// Appends positioned text to a scene, preserving each run's foreground color.
    ///
    /// Positions are relative to the paragraph origin. Use the builder's transform
    /// and clip scopes to position text within its retained control. Only glyph
    /// positions and variation coordinates are copied; font bytes stay shared.
    ///
    /// Missing `.notdef` glyphs are drawn. Unshaped bytes and unsupported synthetic
    /// styles are errors. A scene-geometry error can leave earlier runs appended;
    /// discard that record if the host requires the entire paragraph to be atomic.
    pub fn paint(&self, builder: &mut SceneBuilder) -> Result<(), PaintError> {
        paint_layout(&self.layout, self.diagnostics, builder, None)
    }

    /// Appends text using one foreground color without reshaping the paragraph.
    ///
    /// This supports foreground-only theme changes. Geometry, font selection and
    /// the paragraph's original styling are unchanged. See [`Self::paint`] for
    /// supported behavior and error handling.
    pub fn paint_with_color(
        &self,
        builder: &mut SceneBuilder,
        color: Color,
    ) -> Result<(), PaintError> {
        paint_layout(&self.layout, self.diagnostics, builder, Some(color))
    }
}

pub(crate) fn paint_layout(
    layout: &Layout<Color>,
    diagnostics: TextDiagnostics,
    builder: &mut SceneBuilder,
    color: Option<Color>,
) -> Result<(), PaintError> {
    if diagnostics.unshaped_bytes != 0 {
        return Err(PaintError::MissingFont);
    }
    for line in layout.lines() {
        for run in line.runs() {
            if run.synthesis().embolden() || run.synthesis().skew().is_some() {
                return Err(PaintError::SyntheticStyle);
            }
        }
    }
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(positioned) = item else {
                continue;
            };
            let run = positioned.run();
            let glyphs = positioned
                .positioned_glyphs()
                .map(|glyph| {
                    Ok(Glyph {
                        id: glyph.id.try_into().map_err(|_| PaintError::GlyphIndex)?,
                        position: Point::new(glyph.x, glyph.y),
                    })
                })
                .collect::<Result<Vec<_>, PaintError>>()?;
            builder.glyphs(GlyphRun::new(
                run.font().clone(),
                run.font_size(),
                color.unwrap_or(positioned.style().brush),
                run.normalized_coords().to_vec(),
                glyphs,
            )?)?;
        }
    }
    Ok(())
}

/// A paragraph cannot be completely represented by the scene text operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintError {
    /// Some text has no selected font; inspect [`Paragraph::diagnostics`].
    MissingFont,
    /// Synthetic bold or oblique rendering is not implemented.
    SyntheticStyle,
    /// A glyph index does not fit the OpenType 16-bit glyph-index range.
    GlyphIndex,
    /// Invalid scene geometry or coordinates.
    Scene(SceneError),
}

impl From<SceneError> for PaintError {
    fn from(value: SceneError) -> Self {
        Self::Scene(value)
    }
}

impl fmt::Display for PaintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFont => f.write_str("paragraph contains text with no selected font"),
            Self::SyntheticStyle => f.write_str("synthetic font styling is not supported"),
            Self::GlyphIndex => f.write_str("glyph index exceeds the OpenType range"),
            Self::Scene(error) => error.fmt(f),
        }
    }
}

impl Error for PaintError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Scene(error) => Some(error),
            _ => None,
        }
    }
}
