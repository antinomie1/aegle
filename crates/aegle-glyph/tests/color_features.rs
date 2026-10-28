//! Color representations do not replace the platform rasterizer.
//!
//! `colrv1` and `svg` add per-glyph representations that Skrifa and resvg decode.
//! They used to enable the `swash` feature, which swapped the Linux, Windows and
//! macOS platform rasterizer for the Rust one — so asking for SVG support
//! silently changed every glyph's pixels. This test pins the separation: the
//! color modules render on their own, and the platform rasterizer still handles
//! ordinary outlines in the same build.

use aegle_glyph::{Blob, CacheLimits, Content, FontData, GlyphCache, RasterOptions};

/// The glyph id of `ch` in `font`, read through Skrifa.
fn glyph_id(font: &FontData, ch: char) -> u16 {
    use skrifa::MetadataProvider;
    let parsed = skrifa::FontRef::from_index(font.data.data(), font.index).unwrap();
    u16::try_from(parsed.charmap().map(ch).expect("glyph for char").to_u32()).unwrap()
}

fn cache() -> GlyphCache {
    GlyphCache::with_limits(CacheLimits {
        image_bytes: 1 << 20,
        entries: 16,
        bitmap_bytes: 1 << 20,
    })
}

#[test]
fn ordinary_outlines_still_rasterize_when_a_color_feature_is_on() {
    // The CJK test font has no color representation, so this exercises the
    // platform rasterizer whatever features are enabled.
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec()),
        0,
    );
    let mut cache = cache();
    let options = RasterOptions {
        size: 24.0,
        ..Default::default()
    };
    let glyph = cache
        .rasterize(&font.clone().into(), 103, options)
        .expect("rasterize a plain outline");
    assert_eq!(glyph.content, Content::Mask);
    assert!(glyph.placement.width > 0 && glyph.placement.height > 0);
    assert_eq!(
        glyph.data.len(),
        (glyph.placement.width * glyph.placement.height) as usize
    );
}

#[cfg(feature = "svg")]
#[test]
fn svg_glyphs_render_while_plain_glyphs_keep_their_outline() {
    // One cache, one font with an SVG glyph: the SVG glyph is a color bitmap and
    // the plain outline in the same font is still a mask, in the same build.
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-svg.ttf").to_vec()),
        0,
    );
    let mut cache = cache();
    let options = RasterOptions {
        size: 100.0,
        hint: false,
        ..Default::default()
    };
    let glyph = cache
        .rasterize(&font.clone().into(), glyph_id(&font, 'A'), options)
        .expect("render the SVG glyph");
    assert_eq!(glyph.content, Content::Color);
}

#[cfg(feature = "colrv1")]
#[test]
fn colrv1_glyphs_render_as_color_bitmaps() {
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-colrv1.ttf").to_vec()),
        0,
    );
    let mut cache = cache();
    let options = RasterOptions {
        size: 64.0,
        hint: false,
        ..Default::default()
    };
    let glyph = cache
        .rasterize(&font.clone().into(), glyph_id(&font, 'A'), options)
        .expect("render the COLRv1 glyph");
    assert_eq!(glyph.content, Content::Color);
}

#[cfg(not(any(feature = "colrv1", feature = "svg")))]
#[test]
fn a_color_glyph_without_its_feature_is_unsupported_not_monochrome() {
    // Without either feature a color glyph must not fall back to its outline:
    // that would draw the wrong pixels instead of reporting the gap.
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-svg.ttf").to_vec()),
        0,
    );
    let mut cache = cache();
    let options = RasterOptions {
        size: 100.0,
        hint: false,
        ..Default::default()
    };
    match cache.rasterize(&font.clone().into(), glyph_id(&font, 'A'), options) {
        Err(aegle_glyph::GlyphError::UnsupportedGlyph) => {}
        other => panic!("expected UnsupportedGlyph, got {other:?}"),
    }
}
