//! Scene recording: DirectWrite glyph runs as positioned scene runs.
//!
//! The runs themselves are captured by `runs.rs`, which stores
//! backend-independent [`aegle_scene::SystemFont`] handles; this module only
//! turns them into scene commands, with the same synthetic bold rule as the
//! Pango backend.
#![allow(unsafe_code)]

use aegle_scene::{Color, FontSource, Glyph, GlyphRun, Point, SceneBuilder};

use super::layout::DirectWriteLayout;

impl DirectWriteLayout {
    /// Records the retained runs into a scene.
    pub fn paint(&self, builder: &mut SceneBuilder, color: Color, weight: f32, origin: Point) {
        let bold = weight >= 600.0;
        for run in &self.runs {
            let glyphs = run
                .glyphs
                .iter()
                .map(|&(id, x, y)| Glyph {
                    id,
                    position: Point::new(origin.x + x, origin.y + y),
                })
                .collect();
            builder.glyphs(
                GlyphRun::new(
                    FontSource::System(run.font),
                    run.size,
                    color,
                    Vec::new(),
                    glyphs,
                )
                .synthesized(bold, 0),
            );
        }
    }
}
