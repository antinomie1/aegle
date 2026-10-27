//! UTF-16 ↔ UTF-8 offset conversion for platform text backends.
//!
//! DirectWrite and CoreText both index text in UTF-16 code units, while Aegle's
//! public offsets are UTF-8 bytes that must split neither a codepoint nor (for
//! caret positions) a character. These helpers convert in both directions and
//! snap down to the containing character, so a platform hit test can never
//! produce a byte offset inside a multi-byte character — the bug that made the
//! host publish invalid IME surrounding ranges.
//!
//! Platform backends use these at their boundary; they are public because a
//! host writing its own platform adapter needs the same conversion.

/// Converts a UTF-8 byte offset to a UTF-16 code-unit offset.
///
/// Offsets past the end clamp to the length; an offset inside a character snaps
/// down to that character's start, matching [`snap`].
pub fn to_utf16(text: &str, byte: usize) -> u32 {
    let byte = snap(text, byte);
    let units: usize = text[..byte].chars().map(char::len_utf16).sum();
    units as u32
}

/// Converts a UTF-16 code-unit offset to a UTF-8 byte offset.
///
/// An offset in the middle of a surrogate pair snaps down to the pair's start;
/// an offset past the end clamps to the text length.
pub fn to_utf8(text: &str, unit: u32) -> usize {
    let mut remaining = unit as usize;
    for (byte, ch) in text.char_indices() {
        let width = ch.len_utf16();
        if remaining < width {
            return byte;
        }
        remaining -= width;
    }
    text.len()
}

/// Snaps a UTF-8 byte offset down to a character boundary and clamps it.
pub fn snap(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// The byte offset after the character that starts at `byte`.
pub fn after(text: &str, byte: usize) -> usize {
    let byte = snap(text, byte);
    byte + text[byte..].chars().next().map_or(0, char::len_utf8)
}

/// The byte offset of the character before `byte`.
pub fn before(text: &str, byte: usize) -> usize {
    let byte = snap(text, byte);
    text[..byte]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

/// Applies a platform hit test's `(position, trailing)` pair to `text`.
///
/// `position` is a UTF-16 code-unit offset; `trailing` means the point fell in
/// the second half of the character at `position`, so the caret belongs after
/// it. The result is always a UTF-8 character boundary.
pub fn caret(text: &str, position: u32, trailing: bool) -> usize {
    let byte = to_utf8(text, position);
    match trailing {
        true => after(text, byte),
        false => byte,
    }
}
