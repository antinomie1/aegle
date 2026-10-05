//! Scroll geometry shared by scroll views, editors and popups.
use aegle_types::Rect;

/// Scroll distance that brings `[start, start + length)` inside the viewport
/// `[origin, origin + extent)`, or zero when it already fits. A target longer
/// than the viewport aligns its start.
pub fn reveal_delta(start: f32, length: f32, origin: f32, extent: f32) -> f32 {
    if start < origin {
        start - origin
    } else {
        (start + length.min(extent) - origin - extent).max(0.0)
    }
}

/// The overlap of two rectangles, clamped to a finite, possibly empty rectangle
/// inside `a`.
pub fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.origin.x.max(b.origin.x).min(a.origin.x + a.size.width);
    let y = a.origin.y.max(b.origin.y).min(a.origin.y + a.size.height);
    Rect::new(
        x,
        y,
        ((a.origin.x + a.size.width).min(b.origin.x + b.size.width) - x).max(0.0),
        ((a.origin.y + a.size.height).min(b.origin.y + b.size.height) - y).max(0.0),
    )
}

/// A clipped-out IME caret remains a finite, zero-area anchor at the nearest edge.
pub fn clamp_anchor(rect: Rect, viewport: Rect) -> Rect {
    let left = viewport.origin.x;
    let top = viewport.origin.y;
    let right = left + viewport.size.width;
    let bottom = top + viewport.size.height;
    let x = rect.origin.x.clamp(left, right);
    let y = rect.origin.y.clamp(top, bottom);
    Rect::new(
        x,
        y,
        ((rect.origin.x + rect.size.width).min(right) - x).max(0.0),
        ((rect.origin.y + rect.size.height).min(bottom) - y).max(0.0),
    )
}
