//! Popup placement: the requested side, a flip when only the other side
//! fits, and a slide onto the screen.

use aegle_platform::{Point, Rect, Side};
use aegle_platform_macos::popup_origin;
use aegle_types::Size;

const SCREEN: Rect = Rect::new(0.0, 25.0, 1000.0, 775.0);

#[test]
fn popups_open_beside_their_anchor_and_stay_on_screen() {
    let anchor = Rect::new(100.0, 100.0, 80.0, 20.0);
    let menu = Size::new(120.0, 200.0);
    // Each side aligns the popup's start with the anchor's.
    assert_eq!(
        popup_origin(anchor, menu, Side::Below, SCREEN),
        Point::new(100.0, 120.0)
    );
    assert_eq!(
        popup_origin(anchor, Size::new(120.0, 50.0), Side::Above, SCREEN),
        Point::new(100.0, 50.0)
    );
    assert_eq!(
        popup_origin(anchor, menu, Side::Right, SCREEN),
        Point::new(180.0, 100.0)
    );
    // Too little room above flips below; left of the screen flips right.
    assert_eq!(
        popup_origin(anchor, menu, Side::Above, SCREEN),
        Point::new(100.0, 120.0)
    );
    assert_eq!(
        popup_origin(anchor, menu, Side::Left, SCREEN),
        Point::new(180.0, 100.0)
    );

    // Near the bottom-right corner: a dropdown flips up and slides left,
    // a submenu flips left and slides up.
    let corner = Rect::new(950.0, 700.0, 40.0, 20.0);
    assert_eq!(
        popup_origin(corner, menu, Side::Below, SCREEN),
        Point::new(880.0, 500.0)
    );
    assert_eq!(
        popup_origin(corner, menu, Side::Right, SCREEN),
        Point::new(830.0, 600.0)
    );

    // Neither side fits: the popup slides onto the screen instead, and one
    // larger than the screen keeps its top-left corner on it.
    let tall = Size::new(100.0, 700.0);
    let middle = Rect::new(400.0, 400.0, 40.0, 20.0);
    assert_eq!(
        popup_origin(middle, tall, Side::Below, SCREEN),
        Point::new(400.0, 100.0)
    );
    assert_eq!(
        popup_origin(middle, Size::new(2000.0, 2000.0), Side::Below, SCREEN),
        Point::new(0.0, 25.0)
    );
}
