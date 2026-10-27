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

#[test]
fn line_height_reduces_all_three_forms_to_one_target() {
    use aegle_text::LineHeight;
    use aegle_text::offsets::line_height;
    // Natural line 20, font size 16.
    assert_eq!(
        line_height(LineHeight::MetricsRelative(1.5), 20.0, 16.0),
        Some(30.0)
    );
    assert_eq!(
        line_height(LineHeight::FontSizeRelative(2.0), 20.0, 16.0),
        Some(32.0)
    );
    assert_eq!(
        line_height(LineHeight::Absolute(24.0), 20.0, 16.0),
        Some(24.0)
    );
    // A natural height the platform cannot provide leaves its spacing alone.
    assert_eq!(line_height(LineHeight::Absolute(24.0), 0.0, 16.0), None);
    assert_eq!(
        line_height(LineHeight::Absolute(24.0), f32::NAN, 16.0),
        None
    );
    // A request that would produce a nonpositive or nonfinite target is refused.
    assert_eq!(line_height(LineHeight::Absolute(0.0), 20.0, 16.0), None);
    assert_eq!(
        line_height(LineHeight::FontSizeRelative(f32::INFINITY), 20.0, 16.0),
        None
    );
}

#[test]
fn cluster_stepping_always_advances_and_stops_at_the_edges() {
    use aegle_text::offsets::step_cluster;
    // Four clusters: "ab" + "cd" + "ef" + "gh" in code units.
    let clusters = [(0, 2), (2, 4), (4, 6), (6, 8)];
    // Forward from every position reaches the next cluster end, then stops.
    assert_eq!(step_cluster(&clusters, 0, true), Some(2));
    assert_eq!(step_cluster(&clusters, 1, true), Some(2));
    assert_eq!(step_cluster(&clusters, 2, true), Some(4));
    assert_eq!(step_cluster(&clusters, 7, true), Some(8));
    assert_eq!(step_cluster(&clusters, 8, true), None);
    // Backward from every position reaches the previous cluster start.
    assert_eq!(step_cluster(&clusters, 8, false), Some(6));
    assert_eq!(step_cluster(&clusters, 7, false), Some(6));
    assert_eq!(step_cluster(&clusters, 6, false), Some(4));
    assert_eq!(step_cluster(&clusters, 1, false), Some(0));
    assert_eq!(step_cluster(&clusters, 0, false), None);
    // Repeating a direction never sticks in the middle of the text.
    let mut unit = 0;
    let mut steps = 0;
    while let Some(next) = step_cluster(&clusters, unit, true) {
        assert!(next > unit, "forward step from {unit} reached {next}");
        unit = next;
        steps += 1;
    }
    assert_eq!((unit, steps), (8, 4));
    assert_eq!(step_cluster(&[], 0, true), None);
}
