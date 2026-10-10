use crate::{Layout, Paragraph, TextDiagnostics};
use aegle_scene::{Glyph, GlyphRun, SceneBuilder};
use aegle_types::{Color, Point};
use parley::PositionedLayoutItem;

impl Paragraph {
    /// Appends positioned text to a scene, preserving each run's foreground color.
    ///
    /// Positions are relative to the paragraph origin. Use the builder's transform
    /// and clip scopes to position text within its retained control. Only glyph
    /// positions and variation coordinates are copied; font bytes stay shared.
    ///
    /// Missing `.notdef` glyphs are drawn. Panics when some text has no font at
    /// all (see [`Paragraph::diagnostics`]): register a font before painting.
    pub fn paint(&self, builder: &mut SceneBuilder) {
        paint_layout(&self.layout, self.diagnostics, builder, None, self.weight)
    }

    /// Appends text using one foreground color without reshaping the paragraph.
    ///
    /// This supports foreground-only theme changes. Geometry, font selection and
    /// the paragraph's original styling are unchanged. Panics as [`Self::paint`].
    pub fn paint_with_color(&self, builder: &mut SceneBuilder, color: Color) {
        paint_layout(
            &self.layout,
            self.diagnostics,
            builder,
            Some(color),
            self.weight,
        )
    }
}

pub(crate) fn paint_layout(
    layout: &Layout<Color>,
    diagnostics: TextDiagnostics,
    builder: &mut SceneBuilder,
    color: Option<Color>,
    weight: f32,
) {
    assert!(
        diagnostics.unshaped_bytes == 0,
        "paragraph contains text with no selected font"
    );
    // Fontique suggests emboldening whenever the matched face is lighter than
    // requested, as for a 500 request on a family with only 400 and 700.
    // Like browsers, synthesize only bold (600 and up); a medium request
    // keeps the regular outlines instead of thickened ones.
    let bold = weight >= 600.0;
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(positioned) = item else {
                continue;
            };
            let run = positioned.run();
            // OpenType glyph indices are 16-bit.
            let glyphs = positioned
                .positioned_glyphs()
                .map(|glyph| Glyph {
                    id: glyph.id as u16,
                    position: Point::new(glyph.x, glyph.y),
                })
                .collect();
            let run = GlyphRun::new(
                run.font().clone(),
                run.font_size(),
                color.unwrap_or(positioned.style().brush),
                run.normalized_coords().to_vec(),
                glyphs,
            )
            .synthesized(
                bold && run.synthesis().embolden(),
                run.synthesis().skew().map_or(0, |a| a.round() as i8),
            );
            builder.glyphs(run);
        }
    }
}
