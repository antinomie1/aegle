//! Platform outline rasterizers produce the same mask format as FreeType.
//!
//! The Windows GDI and macOS CoreGraphics backends both feed their outlines
//! through `aegle_glyph::OutlineBuilder`, so this test pins the contract those
//! two backends rely on: coverage bytes, baseline-relative placement, and a
//! stable result for the same outline. The actual GDI/CoreGraphics calls can
//! only run on their own systems; the geometry below is the shared part.

use aegle_glyph::{Content, OutlineBuilder};

/// A 12×8 filled rectangle with a notch, in y-up font space.
fn notched() -> OutlineBuilder {
    styled(false, 0)
}

/// The same shape with synthetic bold and/or a slant.
fn styled(embolden: bool, skew: i8) -> OutlineBuilder {
    let mut builder = OutlineBuilder::new(16.0, embolden, skew);
    builder.move_to(0.0, 0.0);
    builder.line_to(12.0, 0.0);
    builder.line_to(12.0, 8.0);
    builder.line_to(6.0, 8.0);
    builder.line_to(6.0, 4.0);
    builder.line_to(0.0, 4.0);
    builder.close();
    builder
}

#[test]
fn platform_outlines_rasterize_to_baseline_relative_coverage() {
    let image = notched()
        .rasterize([0.0; 2], |bytes| {
            assert_eq!(bytes, 12 * 8, "the reserve request matches the image");
            Ok(())
        })
        .expect("rasterize");
    assert_eq!(image.content, Content::Mask);
    assert_eq!(image.placement.left, 0);
    // The outline is 8 pixels above the baseline, so the mask top is 8.
    assert_eq!(image.placement.top, 8);
    assert_eq!((image.placement.width, image.placement.height), (12, 8));
    assert_eq!(image.data.len(), 12 * 8);
    // The notch at the top-left of the glyph leaves that quadrant empty.
    let at = |x: usize, y: usize| image.data[y * 12 + x];
    assert_eq!(at(1, 1), 0, "notch is empty");
    assert_eq!(at(9, 1), 255, "right of the notch is filled");
    assert_eq!(at(1, 7), 255, "below the notch is filled");
}

#[test]
fn identical_outlines_rasterize_identically() {
    // Both platform backends build the same path from the same font data, so
    // the result must not depend on how the builder was driven.
    let first = notched().rasterize([0.0; 2], |_| Ok(())).unwrap();
    let second = notched().rasterize([0.0; 2], |_| Ok(())).unwrap();
    assert_eq!(first.placement, second.placement);
    assert_eq!(first.data, second.data);
}

#[test]
fn synthetic_style_is_applied_before_rasterizing() {
    let plain = notched().rasterize([0.0; 2], |_| Ok(())).unwrap();
    let bold = styled(true, 0).rasterize([0.0; 2], |_| Ok(())).unwrap();
    assert!(bold.placement.width > plain.placement.width);
    let slanted = styled(false, 20).rasterize([0.0; 2], |_| Ok(())).unwrap();
    assert!(slanted.placement.width > plain.placement.width);
    assert_eq!(slanted.placement.top, plain.placement.top);
}
