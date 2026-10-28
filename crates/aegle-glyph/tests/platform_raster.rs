//! The Windows and macOS glyph backends render system coverage in the mask
//! format the FreeType backend produces on Linux.
//!
//! DirectWrite and CoreText antialias the glyph themselves, so these tests only
//! run on their own systems; they pin the contract the renderers rely on:
//! coverage bytes, baseline-relative placement, empty spaces, the image budget,
//! and that synthetic bold and slant change the pixels.
#![cfg(all(
    any(target_os = "windows", target_os = "macos"),
    not(feature = "swash")
))]

use aegle_glyph::{Blob, CacheLimits, Content, FontData, GlyphCache, GlyphError, RasterOptions};

/// Glyph 103 of the bundled test font is '你'; glyph 1 is a space.
const NI: u16 = 103;
const SPACE: u16 = 1;

fn register() {
    #[cfg(target_os = "windows")]
    let _ = aegle_glyph::directwrite::register();
    #[cfg(target_os = "macos")]
    let _ = aegle_glyph::cg::register();
}

fn font() -> aegle_glyph::FontSource {
    FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec()),
        0,
    )
    .into()
}

fn cache() -> GlyphCache {
    GlyphCache::with_limits(CacheLimits {
        image_bytes: 1 << 20,
        entries: 64,
        bitmap_bytes: 1 << 20,
    })
}

/// Total coverage of one rendered glyph.
fn ink(cache: &mut GlyphCache, options: RasterOptions<'_>) -> (u64, aegle_glyph::Placement) {
    let image = cache.rasterize(&font(), NI, options).expect("rasterize");
    assert_eq!(image.content, Content::Mask);
    assert_eq!(
        image.data.len(),
        (image.placement.width * image.placement.height) as usize
    );
    let total = image.data.iter().map(|&byte| u64::from(byte)).sum();
    (total, image.placement)
}

#[test]
fn system_coverage_matches_the_mask_contract() {
    register();
    let mut cache = cache();
    let options = RasterOptions {
        size: 24.0,
        ..Default::default()
    };
    let (total, placement) = ink(&mut cache, options);
    assert!(total > 0, "the glyph drew no coverage");
    // A CJK ideograph sits on the baseline and rises about one em above it.
    assert!(
        placement.top > 12 && placement.top <= 30,
        "top {}",
        placement.top
    );
    assert!(placement.width > 12 && placement.width <= 30);
    assert!(placement.height > 12 && placement.height <= 30);

    let space = cache.rasterize(&font(), SPACE, options).expect("space");
    assert_eq!((space.placement.width, space.placement.height), (0, 0));
    assert!(space.data.is_empty());

    let huge = RasterOptions {
        size: 8192.0,
        ..options
    };
    let mut small = GlyphCache::with_limits(CacheLimits {
        image_bytes: 65536,
        entries: 16,
        bitmap_bytes: 65536,
    });
    assert_eq!(
        small.rasterize(&font(), NI, huge).unwrap_err(),
        GlyphError::ImageBudget
    );
}

#[test]
fn synthetic_bold_and_slant_change_the_glyph() {
    register();
    let mut cache = cache();
    let plain = RasterOptions {
        size: 32.0,
        hint: false,
        ..Default::default()
    };
    let (regular, upright) = ink(&mut cache, plain);
    let (bold, _) = ink(
        &mut cache,
        RasterOptions {
            embolden: true,
            ..plain
        },
    );
    assert!(bold > regular, "bold {bold} is not heavier than {regular}");
    let (_, slanted) = ink(&mut cache, RasterOptions { skew: 20, ..plain });
    // Leaning right moves the top-right corner further right than the glyph's
    // upright extent.
    assert!(
        slanted.left + slanted.width as i32 > upright.left + upright.width as i32,
        "slanted {slanted:?} upright {upright:?}"
    );
}
