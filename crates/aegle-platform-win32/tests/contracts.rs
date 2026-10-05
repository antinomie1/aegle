//! Input boundary checks independent of a native desktop.
use aegle_platform_win32::{ImeRequest, utf16_cursor};
use aegle_types::Rect;

#[test]
fn ime_boundaries_preserve_unicode_and_fractional_dpi() {
    let text = "A界🙂e\u{301}";
    assert_eq!(utf16_cursor(text, 2).unwrap(), 4);
    assert!(utf16_cursor(text, 3).is_err());
    assert_eq!(utf16_cursor(text, 4).unwrap(), 8);
    assert_eq!(utf16_cursor(text, 6).unwrap(), text.len());
    assert!(utf16_cursor(text, 7).is_err());
    let request = ImeRequest {
        cursor_rect: Rect::new(1.25, -1.0, 0.5, 12.0),
    };
    assert_eq!(request.validate(1.5).unwrap(), [1, -2, 3, 17]);
    assert!(request.validate(f32::NAN).is_err());
    assert!(
        ImeRequest {
            cursor_rect: Rect::new(0.0, 0.0, -1.0, 1.0)
        }
        .validate(1.0)
        .is_err()
    );
}
