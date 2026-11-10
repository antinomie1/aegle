//! The parts of a window's content that move or resize it, and the rule
//! every platform resolves a press against.

use crate::{PixelSize, Point, Rect, ResizeEdge, WindowArea};

/// Logical pixels inside each edge of an undecorated, resizable window that
/// resize it where the window system keeps no border of its own; see
/// [`border_edge`].
pub const RESIZE_BORDER: f32 = 6.0;

/// Logical pixels from a corner, along an edge, that resize both axes.
const RESIZE_CORNER: f32 = 16.0;

/// Checks the rectangles of [`crate::Platform::set_areas`]: finite, with
/// extents that are not negative.
pub fn check_areas(areas: &[(Rect, WindowArea)]) -> crate::Result<()> {
    let valid = |rect: &Rect| {
        [rect.origin.x, rect.origin.y]
            .into_iter()
            .all(f32::is_finite)
            && [rect.size.width, rect.size.height]
                .into_iter()
                .all(|extent| extent.is_finite() && extent >= 0.0)
    };
    match areas.iter().all(|(rect, _)| valid(rect)) {
        true => Ok(()),
        false => Err(crate::Error::InvalidSize),
    }
}

/// The area a press at `point` falls in: that of the last rectangle holding
/// it, so later rectangles lie above earlier ones, and
/// [`WindowArea::Client`] outside every rectangle.
pub fn area_at(areas: &[(Rect, WindowArea)], point: Point) -> WindowArea {
    areas
        .iter()
        .rev()
        .find(|(rect, _)| rect.contains(point))
        .map_or(WindowArea::Client, |(_, area)| *area)
}

/// The edge or corner a press at `point` resizes in the resize border of an
/// undecorated window with content of `size`: within [`RESIZE_BORDER`] of an
/// edge, and a corner when also near the end of that edge. `None` further
/// inside and outside the content.
pub fn border_edge(size: PixelSize, point: Point) -> Option<ResizeEdge> {
    let (width, height) = (size.width as f32, size.height as f32);
    let (left, top) = (point.x, point.y);
    let (right, bottom) = (width - point.x, height - point.y);
    // The right and bottom distances are positive inside the content.
    if left < 0.0 || top < 0.0 || right <= 0.0 || bottom <= 0.0 {
        return None;
    }
    // Half-open like the content: the far sides include their last pixel.
    let near = left < RESIZE_BORDER
        || top < RESIZE_BORDER
        || right <= RESIZE_BORDER
        || bottom <= RESIZE_BORDER;
    if !near {
        return None;
    }
    // Nearest side per axis, when the point is within the corner reach of it.
    let horizontal = match left < right {
        true => (left < RESIZE_CORNER).then_some(ResizeEdge::Left),
        false => (right <= RESIZE_CORNER).then_some(ResizeEdge::Right),
    };
    let vertical = match top < bottom {
        true => (top < RESIZE_CORNER).then_some(ResizeEdge::Top),
        false => (bottom <= RESIZE_CORNER).then_some(ResizeEdge::Bottom),
    };
    Some(match (horizontal, vertical) {
        (Some(ResizeEdge::Left), Some(ResizeEdge::Top)) => ResizeEdge::TopLeft,
        (Some(ResizeEdge::Right), Some(ResizeEdge::Top)) => ResizeEdge::TopRight,
        (Some(ResizeEdge::Left), Some(_)) => ResizeEdge::BottomLeft,
        (Some(ResizeEdge::Right), Some(_)) => ResizeEdge::BottomRight,
        (Some(edge), None) | (None, Some(edge)) => edge,
        _ => unreachable!("a point in the border is near an edge"),
    })
}
