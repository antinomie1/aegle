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
        .rasterize(&font.clone().into(), 103, options)
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
    let space = cache
        .rasterize(&font.clone().into(), 1, options)
        .expect("rasterize space");
    assert_eq!(space.placement.width, 0);
    assert_eq!(space.placement.height, 0);
    assert!(space.data.is_empty());

    // Limits check: huge size exceeds budget
    let huge = RasterOptions {
        size: 8192.0,
        ..options
    };
    assert_eq!(
        cache
            .rasterize(&font.clone().into(), 103, huge)
            .unwrap_err(),
        GlyphError::ImageBudget
    );
}

#[test]
fn freetype_scales_bitmap_only_color_strikes() {
    // One CBDT glyph (U+1F3AF) at a 109 ppem strike, no outlines: FreeType
    // reports no units per em and accepts no other pixel size.
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-bitmap.ttf").to_vec()),
        0,
    );
    let mut cache = GlyphCache::with_limits(CacheLimits {
        image_bytes: 1 << 20,
        entries: 16,
        bitmap_bytes: 1 << 20,
    });
    let mut size = |size: f32| {
        let options = RasterOptions {
            size,
            ..Default::default()
        };
        let image = cache
            .rasterize(&font.clone().into(), 1, options)
            .expect("rasterize a bitmap glyph");
        assert_eq!(image.content, Content::Color);
        let placement = image.placement;
        assert_eq!(
            image.data.len(),
            (placement.width * placement.height * 4) as usize
        );
        assert!(image.data.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
        (placement.width, placement.height, placement.top)
    };
    let strike = size(109.0);
    let small = size(16.0);
    assert_eq!(strike, (136, 128, 101));
    // Scaled from the strike: 16 / 109 of each length.
    assert_eq!(small, (20, 19, 15));
}
