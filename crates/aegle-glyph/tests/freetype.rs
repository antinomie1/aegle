//! Tests for the dynamic FreeType glyph rasterizer on Linux.
#![cfg(target_os = "linux")]

use aegle_glyph::{Blob, CacheLimits, Content, FontData, GlyphCache, GlyphError, RasterOptions};

#[test]
fn freetype_rasterizes_cjk_and_spaces() {
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec()),
        0,
    );

    let mut cache = GlyphCache::with_limits(CacheLimits {
        image_bytes: 65536,
        entries: 16,
        bitmap_bytes: 65536,
    });

    let options = RasterOptions {
        size: 24.0,
        ..Default::default()
    };

    // Glyph 103 in aegle-test-cjk is '你'
    let image = cache
        .rasterize(&font, 103, options)
        .expect("rasterize cjk glyph");
    assert_eq!(image.content, Content::Mask);
    assert!(image.placement.width > 0);
    assert!(image.placement.height > 0);
    assert!(!image.data.is_empty());
    assert_eq!(
        image.data.len(),
        (image.placement.width * image.placement.height) as usize
    );

    // Empty glyph or whitespace (glyph index 1 is space in aegle-test-cjk)
    let space = cache.rasterize(&font, 1, options).expect("rasterize space");
    assert_eq!(space.placement.width, 0);
    assert_eq!(space.placement.height, 0);
    assert!(space.data.is_empty());

    // Limits check: huge size exceeds budget
    let huge = RasterOptions {
        size: 8192.0,
        ..options
    };
    assert_eq!(
        cache.rasterize(&font, 103, huge).unwrap_err(),
        GlyphError::ImageBudget
    );
}
