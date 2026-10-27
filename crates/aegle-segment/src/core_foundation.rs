//! macOS: CoreFoundation's tokenizer with the best language for the text, which
//! uses the dictionaries installed with the operating system.
#![allow(unsafe_code)] // Creating the tokenizer and stepping it are C calls.
use objc2_core_foundation::{
    CFLocale, CFRange, CFString, CFStringTokenizer, kCFStringTokenizerUnitWordBoundary,
};

use crate::units;

/// CoreFoundation is always present on macOS, so the segmenter is too.
pub(super) fn segments() -> Option<super::Segments> {
    Some(segment)
}

fn segment(text: &str) -> Option<Vec<usize>> {
    let string = CFString::from_str(text);
    let length = string.length();
    if length == 0 {
        return None;
    }
    let whole = CFRange {
        location: 0,
        length,
    };
    // The language decides which dictionaries apply; the tokenizer falls back
    // to the user's preferred languages when the text does not say.
    let language = CFStringTokenizer::best_string_language(&string, whole);
    let locale = language.and_then(|language| CFLocale::new(None, Some(&language)));
    // SAFETY: the string and locale outlive the tokenizer, and the range is
    // inside the string.
    let tokenizer = unsafe {
        CFStringTokenizer::new(
            None,
            Some(&string),
            whole,
            kCFStringTokenizerUnitWordBoundary,
            locale.as_deref(),
        )
    }?;
    let mut offsets = Vec::new();
    while !tokenizer.advance_to_next_token().is_empty() {
        offsets.push(tokenizer.current_token_range().location as usize);
    }
    Some(units::to_bytes(text, &offsets))
}
