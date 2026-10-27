//! The language hint a segmenter needs to pick its dictionaries.
//!
//! ICU and CoreFoundation select word-breaking dictionaries by language, while
//! the text itself only says which script is used. The first script in the text
//! that has dictionaries is therefore the hint; it needs no locale setup from
//! the application, and getting it wrong only degrades word boundaries.

/// The most characters examined for the hint.
const SCAN: usize = 256;

/// A BCP 47-style language code for the text, or `None` when no script in it
/// uses dictionary segmentation.
pub(crate) fn guess(text: &str) -> Option<&'static str> {
    let mut han = false;
    let mut hangul = false;
    for ch in text.chars().take(SCAN) {
        match ch {
            // Kana comes first: Japanese text mixes it with Han characters.
            '\u{3040}'..='\u{30ff}' | '\u{31f0}'..='\u{31ff}' => return Some("ja"),
            '\u{ac00}'..='\u{d7af}' | '\u{1100}'..='\u{11ff}' => hangul = true,
            '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{f900}'..='\u{faff}' => {
                han = true
            }
            '\u{0e00}'..='\u{0e7f}' => return Some("th"),
            '\u{0e80}'..='\u{0eff}' => return Some("lo"),
            '\u{1000}'..='\u{109f}' => return Some("my"),
            '\u{1780}'..='\u{17ff}' => return Some("km"),
            _ => {}
        }
    }
    match (han, hangul) {
        (_, true) => Some("ko"),
        (true, false) => Some("zh"),
        (false, false) => None,
    }
}
