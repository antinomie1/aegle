use crate::{Paragraph, TextDiagnostics, layout::Layout};
use aegle_scene::SceneBuilder;
use aegle_types::{Color, Point};

impl Paragraph {
    /// Appends positioned text to a scene in the paragraph's color.
    ///
    /// Positions are relative to the paragraph origin. Use the builder's transform
    /// and clip scopes to position text within its retained control. Only glyph
    /// positions and variation coordinates are copied; font bytes stay shared.
    ///
    /// Missing `.notdef` glyphs are drawn. Panics when some text has no font at
    /// all (see [`Paragraph::diagnostics`]): register a font before painting;
    /// and while released.
    pub fn paint(&self, builder: &mut SceneBuilder) {
        self.paint_with_color(builder, self.color)
    }

    /// Appends text using one foreground color without reshaping the paragraph.
    ///
    /// This supports foreground-only theme changes. Geometry, font selection and
    /// the paragraph's original styling are unchanged. Panics as [`Self::paint`].
    pub fn paint_with_color(&self, builder: &mut SceneBuilder, color: Color) {
        assert!(!self.released, "paragraph layout was released");
        let weight = self.shaping.weight;
        let origin = Point::default();
        paint_layout(
            &self.layout,
            self.diagnostics,
            builder,
            color,
            weight,
            origin,
        )
    }
}

/// Records `layout`'s glyphs at `origin`, all in `color`.
pub(crate) fn paint_layout(
    layout: &Layout,
    diagnostics: TextDiagnostics,
    builder: &mut SceneBuilder,
    color: Color,
    weight: f32,
    origin: Point,
) {
    assert!(
        diagnostics.unshaped_bytes == 0,
        "paragraph contains text with no selected font"
    );
    layout.paint(builder, color, weight, origin);
}
