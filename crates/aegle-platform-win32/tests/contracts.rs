//! Input boundary checks independent of a native desktop.
use aegle_platform_win32::{ImeRequest, MAX_SURROUNDING};
use aegle_types::Rect;

#[test]
fn ime_requests_check_surrounding_text_and_fractional_dpi() {
    let request = ImeRequest {
        cursor_rect: Rect::new(1.25, -1.0, 0.5, 12.0),
        ..Default::default()
    };
    assert_eq!(request.validate(1.5).unwrap(), [1, -2, 3, 17]);
    assert!(request.validate(f32::NAN).is_err());
    let negative = ImeRequest {
        cursor_rect: Rect::new(0.0, 0.0, -1.0, 1.0),
        ..Default::default()
    };
    assert!(negative.validate(1.0).is_err());

    // Offsets are UTF-8 boundaries of the excerpt, which has a fixed bound.
    let text = |surrounding: &str, anchor, cursor| ImeRequest {
        surrounding: Some(surrounding.into()),
        anchor,
        cursor,
        ..Default::default()
    };
    assert!(text("A界🙂", 1, 8).validate(1.0).is_ok());
    assert!(text("A界🙂", 2, 1).validate(1.0).is_err());
    assert!(text("A界🙂", 0, 9).validate(1.0).is_err());
    assert!(
        text(&"a".repeat(MAX_SURROUNDING), 0, 0)
            .validate(1.0)
            .is_ok()
    );
    assert!(
        text(&"a".repeat(MAX_SURROUNDING + 1), 0, 0)
            .validate(1.0)
            .is_err()
    );
    let without = ImeRequest {
        surrounding: None,
        cursor: 1,
        ..Default::default()
    };
    assert!(without.validate(1.0).is_err());
}
