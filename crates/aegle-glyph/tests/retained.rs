//! Cache lifetime, CJK output and public budget boundaries.

use aegle_glyph::{
    Blob, CacheLimits, CacheStats, Content, FontData, GlyphCache, GlyphError, GlyphKey,
    RasterOptions,
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
    let key = GlyphKey::new(&font, ids[0], options).unwrap();
    let owned = key.to_owned();
    assert_eq!(owned.as_key(), key);
    use std::hash::BuildHasher;
    let hash = std::collections::hash_map::RandomState::new();
    assert_eq!(hash.hash_one(key), hash.hash_one(&owned));
    let recolored = GlyphKey::new(
        &font,
        ids[0],
        RasterOptions {
            offset: [-0.0, 0.0],
            foreground: [255, 0, 0, 255],
            ..options
        },
    )
    .unwrap();
    assert_ne!(key, recolored);
    assert_eq!(key.mask(), recolored.mask());
    let varied = GlyphKey::new(
        &font,
        ids[0],
        RasterOptions {
            normalized_coords: &[100],
            ..options
        },
    )
    .unwrap();
    assert_ne!(key, varied);
    assert_eq!(varied, varied.to_owned().as_key());
    #[cfg(feature = "scene")]
    {
        use aegle_glyph::{Placement, RasterTransform};
        use aegle_scene::{Affine, Point};
        let raster = RasterTransform::new(Affine::scale(2.0, 2.0).unwrap(), 12.0).unwrap();
        assert_eq!(raster.size(), options.size);
        assert!(raster.hint());
        let origin = raster.origin(Point::new(1.12, -0.14)).unwrap();
        assert_eq!(origin.offset(), [0.25, 0.75]);
        assert_eq!(
            origin
                .image_transform(Placement {
                    left: -1,
                    top: 3,
                    width: 5,
                    height: 6,
                })
                .unwrap(),
            Affine::translation(1.0, -4.0).unwrap()
        );
        assert!(matches!(
            raster.origin(Point::new(1_048_576.0, 0.0)),
            Err(GlyphError::Coordinates)
        ));
    }
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
