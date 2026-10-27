//! Conversions between UTF-8 byte offsets and the UTF-16 units AppKit's text
//! input ranges count.
use std::ops::Range;

/// `NSNotFound`, the location AppKit uses for an absent range.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const NOT_FOUND: usize = isize::MAX as usize;

/// The number of UTF-16 units of `text`.
pub(crate) fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset of the UTF-8 offset `byte`, a character boundary.
pub(crate) fn to_units(text: &str, byte: usize) -> usize {
    units(&text[..byte])
}

/// The UTF-8 offset of UTF-16 offset `at`, clamped to the text; an offset
/// inside a surrogate pair rounds down to the start of its character.
pub(crate) fn to_byte(text: &str, at: usize) -> usize {
    let mut seen = 0;
    for (byte, ch) in text.char_indices() {
        let next = seen + ch.len_utf16();
        if next > at {
            return byte;
        }
        seen = next;
    }
    text.len()
}

/// The UTF-16 range of the UTF-8 range `bytes`.
pub(crate) fn to_range(text: &str, bytes: Range<usize>) -> Range<usize> {
    let start = to_units(text, bytes.start);
    start..start + units(&text[bytes])
}
