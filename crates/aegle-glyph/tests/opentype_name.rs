//! Bounded OpenType face description parsed from real test fonts.

use aegle_glyph::describe;

const CJK: &[u8] = include_bytes!("../../../tests/assets/aegle-test-cjk.otf");

#[test]
fn reads_family_and_style_from_the_test_font() {
    let face = describe(CJK, 0).expect("parse face");
    assert_eq!(face.family, "Aegle Test CJK");
    assert_eq!(face.subfamily, "Regular");
    assert_eq!(face.weight, 400);
    assert!(!face.italic);
}

#[test]
fn a_second_face_index_is_out_of_range_for_a_single_font() {
    assert!(describe(CJK, 1).is_none());
}

#[test]
fn truncated_or_foreign_data_is_rejected_without_panicking() {
    // Every prefix must fail cleanly; none may read past its slice.
    for end in (0..CJK.len()).step_by(97) {
        let _ = describe(&CJK[..end], 0);
    }
    assert!(describe(b"", 0).is_none());
    assert!(describe(b"not a font at all", 0).is_none());
    assert!(describe(&[0u8; 64], 0).is_none());
}

#[test]
fn a_table_pointing_outside_the_file_is_rejected() {
    let mut bytes = CJK.to_vec();
    // Corrupt the `name` table's length in the table directory.
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    for i in 0..count {
        let record = 12 + i * 16;
        if &bytes[record..record + 4] == b"name" {
            bytes[record + 12..record + 16].copy_from_slice(&u32::MAX.to_be_bytes());
        }
    }
    assert!(describe(&bytes, 0).is_none());
}

#[test]
fn a_huge_name_record_count_is_rejected() {
    let mut bytes = CJK.to_vec();
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    for i in 0..count {
        let record = 12 + i * 16;
        if &bytes[record..record + 4] == b"name" {
            let offset = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap());
            let at = offset as usize;
            // The `name` table header's count field.
            bytes[at + 2..at + 4].copy_from_slice(&u16::MAX.to_be_bytes());
        }
    }
    assert!(describe(&bytes, 0).is_none());
}
