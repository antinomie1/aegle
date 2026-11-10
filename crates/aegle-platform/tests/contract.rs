//! Contract values every platform relies on, independent of a native desktop.

use aegle_platform::{Anchor, Error, Event, ImeRequest, MAX_SURROUNDING, Rect, SeatId, WindowId};

#[test]
fn ime_requests_check_surrounding_text_offsets_and_geometry() {
    let request = ImeRequest {
        cursor_rect: Rect::new(1.25, -1.0, 0.5, 12.0),
        ..Default::default()
    };
    assert!(request.validate().is_ok());
    for rect in [
        Rect::new(0.0, 0.0, -1.0, 1.0),
        Rect::new(f32::NAN, 0.0, 1.0, 1.0),
    ] {
        let invalid = ImeRequest {
            cursor_rect: rect,
            ..Default::default()
        };
        assert!(matches!(invalid.validate(), Err(Error::InvalidIme(_))));
    }

    // Offsets are UTF-8 boundaries of the excerpt, which has a fixed bound.
    let text = |surrounding: &str, anchor, cursor| ImeRequest {
        surrounding: Some(surrounding.into()),
        anchor,
        cursor,
        ..Default::default()
    };
    assert!(text("A界🙂", 1, 8).validate().is_ok());
    assert!(text("A界🙂", 2, 1).validate().is_err());
    assert!(text("A界🙂", 0, 9).validate().is_err());
    assert!(text("a\0", 0, 0).validate().is_err());
    assert!(text(&"a".repeat(MAX_SURROUNDING), 0, 0).validate().is_ok());
    assert!(
        text(&"a".repeat(MAX_SURROUNDING + 1), 0, 0)
            .validate()
            .is_err()
    );
    let without = ImeRequest {
        surrounding: None,
        cursor: 1,
        ..Default::default()
    };
    assert!(without.validate().is_err());
}

#[test]
fn events_name_their_window_and_anchors_combine() {
    let window = WindowId(7);
    let focus = Event::KeyboardFocus {
        window,
        seat: SeatId(0),
        focused: true,
    };
    assert_eq!(focus.window(), Some(window));
    assert_eq!(Event::Wake.window(), None);

    let panel = Anchor::TOP | Anchor::LEFT | Anchor::RIGHT;
    assert!(panel.contains(Anchor::LEFT | Anchor::RIGHT));
    assert!(!panel.contains(Anchor::BOTTOM));
    assert!(panel.contains(Anchor::NONE));
}

#[test]
fn size_limits_reject_empty_or_inverted_bounds_and_clamp_each_axis() {
    use aegle_platform::{PixelSize, check_size_limits};
    let size = |width, height| PixelSize { width, height };
    assert!(check_size_limits(None, None).is_ok());
    assert!(check_size_limits(Some(size(100, 50)), Some(size(100, 50))).is_ok());
    for (min, max) in [
        (Some(size(0, 50)), None),
        (None, Some(size(10, 0))),
        (Some(size(200, 50)), Some(size(100, 400))),
        (Some(size(50, 500)), Some(size(100, 400))),
    ] {
        assert!(matches!(
            check_size_limits(min, max),
            Err(Error::InvalidSize)
        ));
    }
    let (min, max) = (Some(size(100, 100)), Some(size(400, 300)));
    assert_eq!(size(50, 900).clamp(min, max), size(100, 300));
    assert_eq!(size(250, 200).clamp(min, max), size(250, 200));
    assert_eq!(size(50, 900).clamp(None, max), size(50, 300));
}

#[test]
fn later_areas_lie_above_earlier_ones_and_validate_their_rectangles() {
    use aegle_platform::{Point, ResizeEdge, WindowArea, area_at, check_areas};
    // A title bar with a button in it and a grip in the corner below.
    let areas = [
        (Rect::new(0.0, 0.0, 300.0, 32.0), WindowArea::Move),
        (Rect::new(260.0, 0.0, 40.0, 32.0), WindowArea::Client),
        (
            Rect::new(284.0, 184.0, 16.0, 16.0),
            WindowArea::Resize(ResizeEdge::BottomRight),
        ),
    ];
    assert!(check_areas(&areas).is_ok());
    let at = |x, y| area_at(&areas, Point::new(x, y));
    assert_eq!(at(10.0, 10.0), WindowArea::Move);
    assert_eq!(at(270.0, 10.0), WindowArea::Client);
    assert_eq!(at(10.0, 32.0), WindowArea::Client);
    assert_eq!(
        at(290.0, 190.0),
        WindowArea::Resize(ResizeEdge::BottomRight)
    );
    assert_eq!(area_at(&[], Point::new(1.0, 1.0)), WindowArea::Client);

    for rect in [
        Rect::new(f32::NAN, 0.0, 1.0, 1.0),
        Rect::new(0.0, 0.0, -1.0, 1.0),
        Rect::new(0.0, 0.0, 1.0, f32::INFINITY),
    ] {
        assert!(matches!(
            check_areas(&[(rect, WindowArea::Move)]),
            Err(Error::InvalidSize)
        ));
    }
}

#[test]
fn the_resize_border_names_edges_and_corners_inside_the_content() {
    use aegle_platform::{PixelSize, Point, RESIZE_BORDER, ResizeEdge, border_edge};
    let size = PixelSize {
        width: 400,
        height: 300,
    };
    let at = |x, y| border_edge(size, Point::new(x, y));
    assert_eq!(at(200.0, 150.0), None);
    assert_eq!(at(2.0, 150.0), Some(ResizeEdge::Left));
    assert_eq!(at(398.0, 150.0), Some(ResizeEdge::Right));
    assert_eq!(at(200.0, 1.0), Some(ResizeEdge::Top));
    assert_eq!(at(200.0, 299.0), Some(ResizeEdge::Bottom));
    // Corners reach further along the edge than the border is thick.
    assert_eq!(at(2.0, 12.0), Some(ResizeEdge::TopLeft));
    assert_eq!(at(390.0, 2.0), Some(ResizeEdge::TopRight));
    assert_eq!(at(3.0, 290.0), Some(ResizeEdge::BottomLeft));
    assert_eq!(at(399.0, 299.0), Some(ResizeEdge::BottomRight));
    assert_eq!(at(12.0, 12.0), None);
    // Both sides are as thick, in whole pixels too, and end with the content.
    assert_eq!(at(RESIZE_BORDER - 1.0, 150.0), Some(ResizeEdge::Left));
    assert_eq!(at(RESIZE_BORDER, 150.0), None);
    assert_eq!(at(400.0 - RESIZE_BORDER, 150.0), Some(ResizeEdge::Right));
    assert_eq!(at(399.0 - RESIZE_BORDER, 150.0), None);
    assert_eq!(at(-1.0, 150.0), None);
    assert_eq!(at(400.0, 150.0), None);
    assert_eq!(at(200.0, 300.0), None);
}

#[test]
fn icons_hold_exactly_their_pixels() {
    use aegle_platform::Icon;
    let pixels = [0u8; 2 * 3 * 4];
    let icon = |width, height, rgba| Icon {
        width,
        height,
        rgba,
    };
    assert!(icon(2, 3, &pixels).is_valid());
    assert!(!icon(3, 3, &pixels).is_valid());
    assert!(!icon(0, 3, &[]).is_valid());
    assert!(!icon(u32::MAX, u32::MAX, &pixels).is_valid());
}
