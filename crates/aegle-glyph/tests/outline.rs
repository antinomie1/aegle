//! Shared outline → coverage rasterization used by the platform glyph backends.

use aegle_glyph::{Content, GlyphError, OutlineBuilder};

/// Records a filled axis-aligned square from `(0, 0)` to `(size, size)` in y-up space.
fn square(builder: &mut OutlineBuilder, x: f32, y: f32, size: f32) {
    builder.move_to(x, y);
    builder.line_to(x + size, y);
    builder.line_to(x + size, y + size);
    builder.line_to(x, y + size);
    builder.close();
}

#[test]
fn square_covers_its_area_with_baseline_placement() {
    let mut outline = OutlineBuilder::new(16.0, false, 0);
    square(&mut outline, 0.0, 0.0, 10.0);
    let image = outline
        .rasterize([0.0; 2], |bytes| {
            assert_eq!(bytes, 100);
            Ok(())
        })
        .expect("rasterize");
    assert_eq!(image.content, Content::Mask);
    assert_eq!(image.placement.left, 0);
    assert_eq!(image.placement.top, 10);
    assert_eq!(image.placement.width, 10);
    assert_eq!(image.placement.height, 10);
    assert_eq!(image.data.len(), 100);
    // The interior is fully covered; an integral square needs no anti-aliasing.
    assert!(
        image.data[..90].iter().all(|&c| c == 255),
        "interior coverage"
    );
    assert_eq!(image.data[0], 255);
    assert_eq!(image.data[99], 255);
}

#[test]
fn empty_outline_matches_a_space() {
    let outline = OutlineBuilder::new(16.0, false, 0);
    assert!(outline.is_empty());
    let image = outline
        .rasterize([0.5, 0.25], |bytes| {
            assert_eq!(bytes, 0);
            Ok(())
        })
        .expect("rasterize");
    assert_eq!(image.placement.width, 0);
    assert_eq!(image.placement.height, 0);
    assert!(image.data.is_empty());
}

#[test]
fn offset_shifts_placement_and_fractional_phase_anti_aliases() {
    let mut outline = OutlineBuilder::new(16.0, false, 0);
    square(&mut outline, 0.0, 0.0, 4.0);
    let shifted = outline
        .rasterize([0.25, 0.75], |_| Ok(()))
        .expect("rasterize");
    // x phase 0.25 floors to the same column but blurs the left and right edges.
    assert_eq!(shifted.placement.left, 0);
    assert_eq!(shifted.placement.top, 5);
    assert_eq!(shifted.placement.width, 5);
    assert_eq!(shifted.placement.height, 5);
    let left_edge = shifted.data[2 * 5];
    assert!(
        left_edge > 0 && left_edge < 255,
        "phase coverage {left_edge}"
    );
    assert!(shifted.data.contains(&255));
}

#[test]
fn curves_are_tessellated_into_coverage() {
    // A quarter disc approximated with one cubic: its corner is empty, its arc filled.
    const K: f32 = 0.552_284_8;
    let mut outline = OutlineBuilder::new(20.0, false, 0);
    outline.move_to(0.0, 10.0);
    outline.cubic_to(0.0, 10.0 * K, 10.0 * (1.0 - K), 0.0, 10.0, 0.0);
    outline.line_to(0.0, 0.0);
    outline.close();
    let image = outline.rasterize([0.0; 2], |_| Ok(())).expect("rasterize");
    // Mask rows run downward from the baseline: row 9 is next to the baseline.
    let at = |x: usize, y: usize| image.data[y * image.placement.width as usize + x];
    assert!(
        at(0, 9) > 200,
        "the region between arc and origin is filled"
    );
    assert_eq!(at(8, 0), 0, "the corner outside the arc is empty");
    let edges = image.data.iter().filter(|&&c| c > 0 && c < 255).count();
    assert!(
        edges > 4,
        "the curved boundary is anti-aliased ({edges} edge pixels)"
    );
}

#[test]
fn slant_moves_the_top_of_the_outline_right() {
    let mut upright = OutlineBuilder::new(16.0, false, 0);
    square(&mut upright, 0.0, 0.0, 4.0);
    let upright = upright.rasterize([0.0; 2], |_| Ok(())).unwrap();
    let mut slanted = OutlineBuilder::new(16.0, false, 30);
    square(&mut slanted, 0.0, 0.0, 4.0);
    let slanted = slanted.rasterize([0.0; 2], |_| Ok(())).unwrap();
    // tan(30°) · 4 px ≈ 2.3 px of extra width, all on the right.
    assert_eq!(slanted.placement.left, upright.placement.left);
    assert!(slanted.placement.width >= upright.placement.width + 2);
    assert_eq!(slanted.placement.top, upright.placement.top);
}

#[test]
fn embolden_grows_the_mask_on_every_side() {
    let mut plain = OutlineBuilder::new(32.0, false, 0);
    square(&mut plain, 0.0, 0.0, 8.0);
    let plain = plain.rasterize([0.0; 2], |_| Ok(())).unwrap();
    let mut bold = OutlineBuilder::new(32.0, true, 0);
    square(&mut bold, 0.0, 0.0, 8.0);
    let bold = bold.rasterize([0.0; 2], |_| Ok(())).unwrap();
    // 32 / 32 = 1 px of total growth, half a pixel per side.
    assert!(bold.placement.width > plain.placement.width);
    assert!(bold.placement.height > plain.placement.height);
    let plain_coverage: usize = plain.data.iter().map(|&c| usize::from(c)).sum();
    let bold_coverage: usize = bold.data.iter().map(|&c| usize::from(c)).sum();
    assert!(bold_coverage > plain_coverage);
}

#[test]
fn budget_is_asked_before_allocating() {
    let mut outline = OutlineBuilder::new(16.0, false, 0);
    square(&mut outline, 0.0, 0.0, 10.0);
    let error = outline
        .rasterize([0.0; 2], |_| Err(GlyphError::ImageBudget))
        .unwrap_err();
    assert_eq!(error, GlyphError::ImageBudget);
}

#[test]
fn nonfinite_coordinates_are_rejected() {
    let mut outline = OutlineBuilder::new(16.0, false, 0);
    outline.move_to(f32::NAN, 0.0);
    assert_eq!(
        outline.rasterize([0.0; 2], |_| Ok(())).unwrap_err(),
        GlyphError::Coordinates
    );
}

/// Draws a real glyph's outline through skrifa and rasterizes it with the
/// shared path, so the two platform backends' shared code sees real font data.
#[test]
fn real_font_outline_rasterizes_like_the_reference() {
    use skrifa::{
        FontRef, MetadataProvider,
        instance::{LocationRef, Size},
        outline::{DrawSettings, OutlinePen},
        raw::types::GlyphId,
    };

    struct Pen(OutlineBuilder);
    impl OutlinePen for Pen {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.move_to(x, y);
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.line_to(x, y);
        }
        fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
            self.0.quad_to(cx, cy, x, y);
        }
        fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
            self.0.cubic_to(c1x, c1y, c2x, c2y, x, y);
        }
        fn close(&mut self) {
            self.0.close();
        }
    }

    let bytes = include_bytes!("../../../tests/assets/aegle-test-cjk.otf");
    let font = FontRef::new(bytes).expect("parse font");
    let outlines = font.outline_glyphs();
    let glyph = outlines.get(GlyphId::new(103)).expect("glyph 103");
    let mut pen = Pen(OutlineBuilder::new(24.0, false, 0));
    glyph
        .draw(
            DrawSettings::unhinted(Size::new(24.0), LocationRef::default()),
            &mut pen,
        )
        .expect("draw outline");
    let image = pen
        .0
        .rasterize([0.0; 2], |_| Ok(()))
        .expect("rasterize outline");
    assert_eq!(image.content, Content::Mask);
    assert!(image.placement.width > 8 && image.placement.height > 8);
    let covered = image.data.iter().filter(|&&c| c > 0).count();
    let total = image.data.len();
    assert!(
        covered > total / 4,
        "CJK glyph covers a substantial part of its box: {covered}/{total}"
    );
}
