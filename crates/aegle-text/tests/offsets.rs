//! UTF-16 ↔ UTF-8 offset conversion at platform boundaries.

use aegle_text::offsets::{after, before, caret, snap, to_utf8, to_utf16};

const MIXED: &str = "a你😀b";

#[test]
fn round_trips_ascii_cjk_and_surrogate_pairs() {
    // 'a' 1 B/1 U, '你' 3 B/1 U, '😀' 4 B/2 U, 'b' 1 B/1 U.
    for (byte, unit) in [(0, 0), (1, 1), (4, 2), (8, 4), (9, 5)] {
        assert_eq!(to_utf16(MIXED, byte), unit, "byte {byte}");
        assert_eq!(to_utf8(MIXED, unit), byte, "unit {unit}");
    }
    assert_eq!(to_utf16(MIXED, MIXED.len()), 5);
    assert_eq!(to_utf8(MIXED, 5), MIXED.len());
    assert_eq!(to_utf16(MIXED, 8), 4);
    assert_eq!(to_utf8(MIXED, 4), 8);
}

#[test]
fn interior_offsets_snap_down_to_the_character_start() {
    // Byte 2 is inside the three-byte '你'; unit 3 is inside the emoji pair.
    assert_eq!(snap(MIXED, 2), 1);
    assert_eq!(to_utf16(MIXED, 2), 1);
    assert_eq!(to_utf8(MIXED, 3), 4);
    assert_eq!(to_utf8(MIXED, 4), 8);
    assert_eq!(snap(MIXED, 99), MIXED.len());
    assert_eq!(to_utf8(MIXED, 99), MIXED.len());
}

#[test]
fn trailing_hit_moves_past_the_character() {
    assert_eq!(caret(MIXED, 1, false), 1);
    assert_eq!(caret(MIXED, 1, true), 4);
    assert_eq!(caret(MIXED, 2, false), 4);
    assert_eq!(caret(MIXED, 2, true), 8);
    // A trailing hit on the emoji's low surrogate stops after the emoji.
    assert_eq!(caret(MIXED, 3, true), 8);
    // A trailing hit on the last character stops at the end.
    assert_eq!(caret(MIXED, 4, true), MIXED.len());
    for unit in 0..=5 {
        for trailing in [false, true] {
            assert!(MIXED.is_char_boundary(caret(MIXED, unit, trailing)));
        }
    }
}

#[test]
fn neighbors_stop_at_the_edges() {
    assert_eq!(before(MIXED, 0), 0);
    assert_eq!(before(MIXED, 4), 1);
    assert_eq!(before(MIXED, 1), 0);
    assert_eq!(after(MIXED, MIXED.len()), MIXED.len());
    assert_eq!(after(MIXED, 8), MIXED.len());
}

#[test]
fn paragraph_direction_uses_the_first_strong_character() {
    use aegle_text::offsets::first_strong_rtl;
    assert!(!first_strong_rtl("Aegle 你好"));
    assert!(!first_strong_rtl("你好 Aegle"));
    assert!(first_strong_rtl("שלום Aegle"));
    assert!(first_strong_rtl("مرحبا world"));
    // Leading digits, punctuation and spaces are skipped.
    assert!(!first_strong_rtl("123 \"Aegle\""));
    assert!(first_strong_rtl("42 שלום"));
    // No strong character at all: the paragraph stays left to right.
    assert!(!first_strong_rtl("123 456"));
    assert!(!first_strong_rtl(""));
}
