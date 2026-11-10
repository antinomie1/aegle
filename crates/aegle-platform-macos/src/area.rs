//! Window areas: content that moves or resizes the window when pressed, and
//! the resize border of an undecorated window.
//!
//! A press in a move area hands the drag to the window server. AppKit
//! resizes only titled windows by their edges and has no call that begins a
//! resize, so a press in a resize area or in an undecorated window's border
//! is tracked here: each drag event sets the frame the pointer's travel
//! since the press asks for, within the window's size limits. The pointer
//! shows the frame resize cursor over both.
#![allow(unsafe_code)] // Asking the cursor class what it responds to.

use crate::{MacOs, native::Native};
use aegle_platform::{
    Cursor, PixelSize, Rect, ResizeEdge, Result, WindowArea, WindowId, WindowMode, area_at,
    border_edge, check_areas,
};
use objc2::{ClassType, rc::Retained, sel};
use objc2_app_kit::{
    NSCursor, NSCursorFrameResizeDirections, NSCursorFrameResizePosition, NSEvent, NSEventType,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// A resize the pointer is dragging.
#[derive(Clone, Copy)]
pub(crate) struct Resizing {
    edge: ResizeEdge,
    /// Where the press happened on the screen.
    from: NSPoint,
    /// The window's frame at the press.
    frame: NSRect,
}

/// Whether dragging `edge` moves the window's left, right, top and bottom side.
fn sides(edge: ResizeEdge) -> (bool, bool, bool, bool) {
    use ResizeEdge::*;
    (
        matches!(edge, Left | TopLeft | BottomLeft),
        matches!(edge, Right | TopRight | BottomRight),
        matches!(edge, Top | TopLeft | TopRight),
        matches!(edge, Bottom | BottomLeft | BottomRight),
    )
}

/// The cursor for resizing a frame by `edge`. Systems before macOS 15 have
/// cursors for the sides only and keep `fallback` over the corners.
fn resize_cursor(edge: ResizeEdge, fallback: Cursor) -> Retained<NSCursor> {
    use NSCursorFrameResizePosition as Position;
    let frame_resize = sel!(frameResizeCursorFromPosition:inDirections:);
    if NSCursor::class().responds_to(frame_resize) {
        let position = match edge {
            ResizeEdge::Top => Position::Top,
            ResizeEdge::Bottom => Position::Bottom,
            ResizeEdge::Left => Position::Left,
            ResizeEdge::Right => Position::Right,
            ResizeEdge::TopLeft => Position::TopLeft,
            ResizeEdge::TopRight => Position::TopRight,
            ResizeEdge::BottomLeft => Position::BottomLeft,
            ResizeEdge::BottomRight => Position::BottomRight,
        };
        return NSCursor::frameResizeCursorFromPosition_inDirections(
            position,
            NSCursorFrameResizeDirections::All,
        );
    }
    crate::window::cursor(match edge {
        ResizeEdge::Left | ResizeEdge::Right => Cursor::ResizeHorizontal,
        ResizeEdge::Top | ResizeEdge::Bottom => Cursor::ResizeVertical,
        _ => fallback,
    })
}

impl MacOs {
    pub(crate) fn set_areas(&mut self, id: WindowId, areas: &[(Rect, WindowArea)]) -> Result<()> {
        check_areas(areas)?;
        let native = self.ordinary(id)?;
        let mut kept = native.areas.borrow_mut();
        kept.clear();
        kept.extend_from_slice(areas);
        Ok(())
    }
}

impl Native {
    /// The area of the event's location that acts now: none while full
    /// screen, and resizing only on a resizable window in its normal mode,
    /// first by the border of an undecorated one.
    fn acting_area(&self, event: &NSEvent) -> WindowArea {
        let undecorated = !self.window.styleMask().contains(NSWindowStyleMask::Titled);
        if self.parent.is_some() || (self.areas.borrow().is_empty() && !undecorated) {
            return WindowArea::Client;
        }
        let mode = self.mode();
        let resizes = mode == WindowMode::Normal && self.resizable();
        let at = self.position(event.locationInWindow());
        if undecorated
            && resizes
            && let Some(edge) = border_edge(self.info.get().size, at)
        {
            return WindowArea::Resize(edge);
        }
        match area_at(&self.areas.borrow(), at) {
            WindowArea::Move if mode != WindowMode::Fullscreen => WindowArea::Move,
            area @ WindowArea::Resize(_) if resizes => area,
            _ => WindowArea::Client,
        }
    }

    /// Begins the move or resize a primary press in a window area asks for.
    /// Returns whether it took the press, which is then not reported.
    pub(crate) fn press_area(&self, event: &NSEvent) -> bool {
        if event.r#type() != NSEventType::LeftMouseDown {
            return false;
        }
        match self.acting_area(event) {
            WindowArea::Move => self.window.performWindowDragWithEvent(event),
            WindowArea::Resize(edge) => self.resizing.set(Some(Resizing {
                edge,
                from: NSEvent::mouseLocation(),
                frame: self.window.frame(),
            })),
            _ => return false,
        }
        true
    }

    /// Follows the pointer with the resize in progress. Returns whether one
    /// is, so its drag events are not reported.
    pub(crate) fn drag_resize(&self) -> bool {
        let Some(resizing) = self.resizing.get() else {
            return false;
        };
        let now = NSEvent::mouseLocation();
        let (dx, dy) = (now.x - resizing.from.x, now.y - resizing.from.y);
        let (left, right, top, bottom) = sides(resizing.edge);
        let start = resizing.frame;
        let content = self.window.contentRectForFrameRect(start).size;
        let chrome = (
            start.size.width - content.width,
            start.size.height - content.height,
        );
        // Screen coordinates grow upward: the top side follows `dy`.
        let grow = |positive: bool, negative: bool, delta: f64| match (positive, negative) {
            (true, _) => delta,
            (_, true) => -delta,
            _ => 0.0,
        };
        let (min, max) = self.limits.get();
        let wanted = PixelSize {
            width: (content.width + grow(right, left, dx)).round().max(1.0) as u32,
            height: (content.height + grow(top, bottom, dy)).round().max(1.0) as u32,
        }
        .clamp(min, max);
        let size = NSSize::new(
            f64::from(wanted.width) + chrome.0,
            f64::from(wanted.height) + chrome.1,
        );
        // The side opposite the dragged one stays where it was.
        let x = match left {
            true => start.origin.x + start.size.width - size.width,
            false => start.origin.x,
        };
        let y = match bottom {
            true => start.origin.y + start.size.height - size.height,
            false => start.origin.y,
        };
        self.window
            .setFrame_display(NSRect::new(NSPoint::new(x, y), size), true);
        true
    }

    /// Ends the resize in progress. Returns whether there was one, so the
    /// release is not reported.
    pub(crate) fn end_resize(&self) -> bool {
        self.resizing.take().is_some()
    }

    /// The cursor over the content: the resize cursor of the resize area
    /// under the pointer, else the window's own shape.
    pub(crate) fn shown_cursor(&self) -> Retained<NSCursor> {
        match self.resize_cursor.get() {
            Some(edge) => resize_cursor(edge, self.cursor.get()),
            None => crate::window::cursor(self.cursor.get()),
        }
    }

    /// Shows the resize cursor while the pointer is over a resize area and
    /// the window's own shape again when it leaves.
    pub(crate) fn hover_area(&self, event: &NSEvent) {
        let edge = match self.acting_area(event) {
            WindowArea::Resize(edge) => Some(edge),
            _ => None,
        };
        if self.resize_cursor.replace(edge) != edge {
            // The view's cursor rectangle would bring the old shape back.
            self.window.invalidateCursorRectsForView(&self.view);
            if self.inside.get() {
                self.shown_cursor().set();
            }
        }
    }
}
