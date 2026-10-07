//! A damage region keeps separate changes apart, merges touching ones and
//! stays within four rectangles.

use aegle_types::{PixelRect, Rect, Region};

fn px(x: u32, y: u32, width: u32, height: u32) -> PixelRect {
    PixelRect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn regions_merge_touching_and_cheapest_rectangles() {
    let mut region = Region::default();
    region.add(px(0, 0, 10, 10));
    region.add(px(100, 100, 10, 10));
    region.add(px(0, 0, 0, 5));
    assert_eq!(region.rects(), &[px(0, 0, 10, 10), px(100, 100, 10, 10)]);
    // Touching rectangles merge, also transitively.
    region.add(px(10, 0, 5, 10));
    region.add(px(15, 0, 85, 100));
    assert_eq!(region.rects(), &[px(0, 0, 110, 110)]);

    let mut spread = Region::default();
    for (x, y) in [(0, 0), (50, 0), (0, 50), (50, 50)] {
        spread.add(px(x, y, 10, 10));
    }
    assert_eq!(spread.rects().len(), 4);
    // A fifth merges with the rectangle whose union grows least.
    spread.add(px(52, 62, 4, 4));
    spread.add(px(200, 200, 1, 1));
    assert_eq!(spread.rects().len(), 4);
    assert!(spread.rects().contains(&px(0, 0, 10, 10)));

    let mut logical = Region::default();
    logical.add(Rect::new(0.5, 0.5, 2.0, 2.0));
    logical.add(Rect::new(2.5, 0.5, 1.0, 1.0));
    assert_eq!(logical.rects(), &[Rect::new(0.5, 0.5, 3.0, 2.0)]);
}
