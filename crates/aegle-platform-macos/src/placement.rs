//! Where a popup goes beside its anchor, as `xdg_positioner` places one:
//! on the requested side, flipped to the opposite side when only that one
//! fits on the screen, then slid along both axes into the screen.
//!
//! AppKit positions windows itself, so the macOS backend computes this; the
//! function is platform-independent so its rules are tested anywhere.

use aegle_platform::{Point, Rect, Side};
use aegle_types::Size;

/// The top-left corner of a popup of `size` opened on `side` of `anchor`,
/// kept inside `bounds`. All three share one top-left-origin, y-down space,
/// such as the desktop in logical pixels; `bounds` is the usable area of the
/// screen showing the anchor.
///
/// The popup's start aligns with the anchor's start ([`Side`]). It moves to
/// the opposite side only when it overflows `bounds` on the requested side
/// and fits on the other one. It then slides into `bounds` on both axes; a
/// popup larger than `bounds` keeps its top-left corner inside them.
pub fn popup_origin(anchor: Rect, size: Size, side: Side, bounds: Rect) -> Point {
    let (left, top) = (anchor.origin.x, anchor.origin.y);
    let (right, bottom) = (left + anchor.size.width, top + anchor.size.height);
    let (min_x, min_y) = (bounds.origin.x, bounds.origin.y);
    let (max_x, max_y) = (min_x + bounds.size.width, min_y + bounds.size.height);
    let (width, height) = (size.width, size.height);
    let (x, y) = match side {
        Side::Below if bottom + height > max_y && top - height >= min_y => (left, top - height),
        Side::Below => (left, bottom),
        Side::Above if top - height < min_y && bottom + height <= max_y => (left, bottom),
        Side::Above => (left, top - height),
        Side::Right if right + width > max_x && left - width >= min_x => (left - width, top),
        Side::Right => (right, top),
        Side::Left if left - width < min_x && right + width <= max_x => (right, top),
        Side::Left => (left - width, top),
    };
    // `max` last, so an oversized popup keeps its start on the screen.
    Point::new(
        x.min(max_x - width).max(min_x),
        y.min(max_y - height).max(min_y),
    )
}
