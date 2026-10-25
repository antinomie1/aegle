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
