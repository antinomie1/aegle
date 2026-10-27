//! Word boundaries from the operating system, and the fallback when there is
//! none. The assertions that need a segmenter are skipped (with a note) on
//! hosts without one, such as a Linux system without libicu.

use aegle_segment::{Boundaries, available, boundaries};

/// The words of `text` as slices, with whitespace dropped.
fn words(text: &str) -> Vec<&str> {
    let bounds = boundaries(text).expect("a segmenter");
    bounds
        .starts()
        .windows(2)
        .map(|pair| &text[pair[0]..pair[1]])
        .filter(|word| !word.trim().is_empty())
        .collect()
}

#[test]
fn boundaries_cover_the_text_in_order() {
    let Some(bounds) = boundaries("Hello, 世界! 你好吗") else {
        eprintln!("no system segmenter here; skipping");
        return;
    };
    let starts = bounds.starts();
    assert_eq!(starts.first(), Some(&0));
    assert_eq!(starts.last(), Some(&"Hello, 世界! 你好吗".len()));
    assert!(starts.windows(2).all(|pair| pair[0] < pair[1]));
    // Every offset is a character boundary, so slicing never panics.
    for &at in starts {
        assert!("Hello, 世界! 你好吗".is_char_boundary(at));
    }
}

#[test]
fn latin_words_split_at_spaces_and_punctuation() {
    if !available() {
        eprintln!("no system segmenter here; skipping");
        return;
    }
    assert_eq!(words("one two three"), ["one", "two", "three"]);
    // Stepping forward stops at word ends, then runs through the spaces.
    let bounds: Boundaries = boundaries("one two").unwrap();
    assert_eq!(bounds.next(0), Some(3));
    assert_eq!(bounds.next(3), Some(7));
    assert_eq!(bounds.next(7), None);
    assert_eq!(bounds.previous(7), Some(4));
    assert_eq!(bounds.previous(4), Some(0));
    assert_eq!(bounds.previous(0), None);
    assert_eq!(bounds.containing(5), 4..7);
}

#[test]
fn cjk_words_come_from_the_system_dictionary() {
    if !available() {
        eprintln!("no system segmenter here; skipping");
        return;
    }
    // Rule-based boundaries alone would report one word for each run of Han
    // characters; the dictionary splits them into compound words.
    let text = "中文分词测试";
    let split = words(text);
    assert!(
        split.len() > 1,
        "expected several words in {text:?}, got {split:?}"
    );
    assert!(split.iter().all(|word| !word.is_empty()));
    // Japanese mixes kana with Han characters and is still segmented.
    let japanese = words("東京都に行きます");
    assert!(japanese.len() > 1, "got {japanese:?}");
    // Thai has no spaces at all; only the dictionary finds word boundaries.
    let thai = words("ภาษาไทยทดสอบ");
    assert!(thai.len() > 1, "got {thai:?}");
}
