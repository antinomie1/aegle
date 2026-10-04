//! Cache lifetime, CJK output and public budget boundaries.

use aegle_glyph::{
    Blob, CacheLimits, CacheStats, Content, FontData, GlyphCache, GlyphError, RasterOptions,
};

#[test]
fn on_demand_cjk_cache_reuse_eviction_and_limits() {
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec()),
        0,
    );
    let parsed = swash::FontRef::from_index(font.data.data(), 0).unwrap();
    let ids = ['你', '好', '界', ' '].map(|c| parsed.charmap().map(c));
    assert!(ids.iter().all(|id| *id != 0));
    let mut cache = GlyphCache::with_limits(CacheLimits {
        image_bytes: 4096,
        entries: 2,
        bitmap_bytes: 4096,
    });
    let options = RasterOptions {
        size: 24.0,
        ..Default::default()
    };
    let image = cache.rasterize(&font, ids[0], options).unwrap();
    assert_eq!(image.content, Content::Mask);
    assert!(image.data.iter().any(|v| *v != 0));
    assert_eq!(
        image.data.len(),
        (image.placement.width * image.placement.height) as usize
    );
    let first = image.data.as_ptr();
    assert_eq!(
        cache
            .rasterize(&font, ids[0], options)
            .unwrap()
            .data
            .as_ptr(),
        first
    );
    let stats = cache.stats();
    cache
        .rasterize(
            &font,
            ids[0],
            RasterOptions {
                foreground: [255, 0, 0, 255],
                ..options
            },
        )
        .unwrap();
    assert_eq!(cache.stats(), stats);
    cache.rasterize(&font, ids[1], options).unwrap();
    cache.rasterize(&font, ids[0], options).unwrap();
    cache.rasterize(&font, ids[2], options).unwrap();
    assert_eq!(cache.stats().entries, 2);
    assert_eq!(
        cache
            .rasterize(&font, ids[0], options)
            .unwrap()
            .data
            .as_ptr(),
        first
    );
    assert!(cache.stats().image_bytes <= 4096);
    assert!(
        cache
            .rasterize(&font, ids[3], options)
            .unwrap()
            .data
            .is_empty()
    );
    assert_eq!(cache.stats().entries, 2);
    let huge = RasterOptions {
        size: 4096.0,
        ..options
    };
    assert_eq!(
        cache.rasterize(&font, ids[0], huge).unwrap_err(),
        GlyphError::ImageBudget
    );
    let invalid = RasterOptions {
        offset: [f32::NAN, 0.0],
        ..options
    };
    assert_eq!(
        cache.rasterize(&font, ids[0], invalid).unwrap_err(),
        GlyphError::InvalidOptions
    );
    let wrong_index = FontData::new(font.data.clone(), 1);
    assert_eq!(
        cache.rasterize(&wrong_index, ids[0], options).unwrap_err(),
        GlyphError::InvalidFont
    );
    let stats = cache.stats();
    cache.release_scratch();
    assert_eq!(cache.stats(), stats);
    cache.clear();
    assert_eq!(cache.stats(), CacheStats::default());

    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-color.otf").to_vec()),
        0,
    );
    let parsed = swash::FontRef::from_index(font.data.data(), 0).unwrap();
    let mut cache = GlyphCache::default();
    let image = cache
        .rasterize(&font, parsed.charmap().map('A'), options)
        .unwrap();
    assert_eq!(image.content, Content::Color);
    assert!(
        image
            .data
            .chunks_exact(4)
            .any(|p| (186..=190).contains(&p[0])
                && p[1] == 0
                && (186..=190).contains(&p[2])
                && p[3] == 255)
    );
    let image = cache
        .rasterize(&font, parsed.charmap().map('B'), RasterOptions::default())
        .unwrap();
    assert_eq!(image.content, Content::Color);
    assert_eq!((image.placement.width, image.placement.height), (2, 2));
    assert_eq!(
        image.data,
        [255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]
    );
    for (size, offset) in [(1e-20, [0.0; 2]), (f32::from_bits(1), [0.25; 2])] {
        let image = cache
            .rasterize(
                &font,
                parsed.charmap().map('B'),
                RasterOptions {
                    size,
                    offset,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(image.data.iter().all(|v| *v == 0));
    }
}
