//! COLRv1 gradients, transforms, composite layers and sweeps in one bitmap.
#![cfg(feature = "colrv1")]
use aegle_glyph::{Blob, CacheLimits, Content, FontData, GlyphCache, GlyphError, RasterOptions};

#[test]
fn paint_graph_resolves_into_a_color_bitmap() {
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-colrv1.ttf").to_vec()),
        0,
    );
    let id = swash::FontRef::from_index(font.data.data(), 0)
        .unwrap()
        .charmap()
        .map('A');
    let mut cache = GlyphCache::with_limits(CacheLimits {
        image_bytes: 1 << 20,
        entries: 16,
        bitmap_bytes: 1 << 20,
    });
    let options = RasterOptions {
        size: 100.0,
        hint: false,
        foreground: [10, 200, 30, 255],
        ..Default::default()
    };
    let glyph = cache.rasterize(&font, id, options).unwrap();
    assert_eq!(glyph.content, Content::Color);
    let placement = glyph.placement;
    // The 1000-unit clip box is 100 px square, resting on the baseline.
    assert_eq!(
        (
            placement.left,
            placement.top,
            placement.width,
            placement.height
        ),
        (0, 100, 100, 100)
    );
    let at = |x: usize, y: usize| -> [u8; 4] {
        glyph.data[(y * 100 + x) * 4..][..4].try_into().unwrap()
    };
    // Linear gradient: red at the left edge, mostly red at x = 10 and bluer later.
    let left = at(10, 90);
    assert!(left[0] > 215 && left[2] < 45 && left[3] == 255, "{left:?}");
    // Upper right, translated and scaled: the foreground color from options.
    assert_eq!(at(75, 25), [10, 200, 30, 255]);
    // Lower right: radial green center multiplied by yellow stays green.
    let center = at(75, 75);
    assert!(
        center[0] < 40 && center[1] > 215 && center[2] < 40,
        "{center:?}"
    );
    // Upper left sweep around (25, 25), counter-clockwise from the right with
    // the file's red to blue; the seam is just below the right-hand ray.
    let right = at(45, 20);
    let above = at(25, 5);
    let left = at(5, 25);
    let below = at(25, 45);
    assert!(right[0] > 230 && right[2] < 30, "{right:?}");
    assert!(
        above[0] > above[2] + 60 && below[2] > below[0] + 60,
        "{above:?} {below:?}"
    );
    assert!(left[0].abs_diff(left[2]) < 30, "{left:?}");

    // The fractional phase shifts the bitmap instead of resampling it.
    let shifted = RasterOptions {
        offset: [0.5, 0.0],
        ..options
    };
    let placement = cache.rasterize(&font, id, shifted).unwrap().placement;
    assert_eq!((placement.left, placement.width), (0, 101));

    let mut tight = GlyphCache::with_limits(CacheLimits {
        image_bytes: 1000,
        entries: 16,
        bitmap_bytes: 1 << 20,
    });
    assert_eq!(
        tight.rasterize(&font, id, options).unwrap_err(),
        GlyphError::ImageBudget
    );
}
