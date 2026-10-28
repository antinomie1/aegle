//! OpenType-SVG glyph documents rasterize through resvg into a color bitmap.
#![cfg(feature = "svg")]
use aegle_glyph::{Blob, CacheLimits, Content, FontData, GlyphCache, RasterOptions};

/// The glyph id of `ch` in `font`, read through Skrifa so the test does not
/// depend on the `swash` feature just to look up a codepoint.
fn glyph_id(font: &FontData, ch: char) -> u16 {
    use skrifa::MetadataProvider;
    let parsed = skrifa::FontRef::from_index(font.data.data(), font.index).unwrap();
    u16::try_from(parsed.charmap().map(ch).expect("glyph for char").to_u32()).unwrap()
}

#[test]
fn svg_glyph_renders_blended_layers_at_the_requested_size() {
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-svg.ttf").to_vec()),
        0,
    );
    let id = glyph_id(&font, 'A');
    let mut cache = GlyphCache::with_limits(CacheLimits {
        image_bytes: 1 << 20,
        entries: 16,
        bitmap_bytes: 1 << 20,
    });
    let options = RasterOptions {
        size: 100.0,
        hint: false,
        ..Default::default()
    };
    let glyph = cache.rasterize(&font.clone().into(), id, options).unwrap();
    assert_eq!(glyph.content, Content::Color);
    let placement = glyph.placement;
    // 1000 × 800 font units: 100 px wide, 80 px above the baseline.
    assert_eq!(
        (
            placement.left,
            placement.top,
            placement.width,
            placement.height
        ),
        (0, 80, 100, 80)
    );
    let at = |x: usize, y: usize| -> [u8; 4] {
        glyph.data[(y * 100 + x) * 4..][..4].try_into().unwrap()
    };
    assert_eq!(at(10, 70), [255, 0, 0, 255]);
    // Half-transparent blue over the red bar blends to purple.
    let blend = at(75, 20);
    assert!(
        blend[0].abs_diff(127) < 3 && blend[2].abs_diff(128) < 3 && blend[3] == 255,
        "{blend:?}"
    );
    let shifted = RasterOptions {
        offset: [0.5, 0.0],
        ..options
    };
    assert_eq!(
        cache
            .rasterize(&font.clone().into(), id, shifted)
            .unwrap()
            .placement
            .width,
        101
    );
}
