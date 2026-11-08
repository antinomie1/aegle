//! The CoreText backend on a real Mac: generic families resolve to installed
//! faces, painted runs sit on their own lines' baselines, and every run's
//! system font rasterizes through CoreGraphics.
#![cfg(all(target_os = "macos", feature = "scene", feature = "system-fonts"))]

use aegle_glyph::{GlyphCache, RasterOptions};
use aegle_scene::{FontSource, SceneBuilder};
use aegle_text::{Alignment, TextStyle, TextSystem};

fn style(families: &str) -> TextStyle<'_> {
    TextStyle {
        families,
        size: 16.0,
        ..TextStyle::default()
    }
}

#[test]
fn monospace_resolves_to_a_fixed_pitch_face() {
    let mut system = TextSystem::system();
    // The first installed family of the list wins, and the generic family
    // means a fixed-pitch face rather than CoreText's proportional default.
    for families in ["monospace", "No Such Family, monospace"] {
        let narrow = system.paragraph("iiiiiiii", &style(families)).unwrap();
        let wide = system.paragraph("MMMMMMMM", &style(families)).unwrap();
        assert_eq!(narrow.size().width, wide.size().width, "{families}");
    }
    let narrow = system.paragraph("iiiiiiii", &style("sans-serif")).unwrap();
    let wide = system.paragraph("MMMMMMMM", &style("sans-serif")).unwrap();
    assert!(narrow.size().width < wide.size().width);
}

#[test]
fn runs_sit_on_their_line_baselines_and_rasterize() {
    let mut system = TextSystem::system();
    let mut paragraph = system
        .paragraph("Aegle 文字 Aegle 文字", &style("sans-serif"))
        .unwrap();
    let one_line = paragraph.size().height;
    paragraph.reflow(Some(60.0), Alignment::Start).unwrap();
    assert!(paragraph.line_count() >= 2);
    let baseline = paragraph.first_baseline().unwrap();
    assert!(baseline > 0.0 && baseline < one_line);

    let mut builder = SceneBuilder::new();
    paragraph.paint(&mut builder);
    let scene = builder.finish();
    let runs = scene.glyph_runs();
    let mut rows: Vec<f32> = runs
        .iter()
        .flat_map(|run| run.glyphs().iter().map(|glyph| glyph.position.y))
        .collect();
    rows.sort_by(f32::total_cmp);
    rows.dedup();
    // Glyph positions are baselines in top-down layout space: the first line's
    // matches the paragraph's, and later lines are a line height further down.
    assert!((rows[0] - baseline).abs() < 0.5, "{rows:?} vs {baseline}");
    assert!(
        rows.iter().any(|&y| y >= baseline + one_line - 0.5),
        "{rows:?}"
    );
    assert!(rows.iter().all(|&y| y <= paragraph.size().height));

    // Both the Latin face and the CJK fallback face rasterize to coverage; a
    // run may hold only the space at a line break.
    let mut cache = GlyphCache::default();
    let mut fonts: Vec<(FontSource, usize)> = Vec::new();
    for run in runs {
        assert!(matches!(run.font(), FontSource::System(_)));
        let options = RasterOptions {
            size: run.size(),
            ..RasterOptions::default()
        };
        let mut covered = 0;
        for glyph in run.glyphs() {
            let image = cache.rasterize(run.font(), glyph.id, options).unwrap();
            covered += image.data.iter().filter(|&&coverage| coverage > 0).count();
        }
        match fonts.iter_mut().find(|(font, _)| font == run.font()) {
            Some((_, total)) => *total += covered,
            None => fonts.push((run.font().clone(), covered)),
        }
    }
    assert!(fonts.len() >= 2, "expected a CJK fallback face");
    assert!(fonts.iter().all(|&(_, covered)| covered > 0), "{fonts:?}");
}
