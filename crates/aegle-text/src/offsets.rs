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

/// Whether a paragraph's first strong character runs right to left.
///
/// Platform layout APIs report bidi levels but not the paragraph direction, and
/// Aegle's alignment defaults need it, so this scans once with the standard
/// first-strong rule: digits, punctuation and unassigned scripts are skipped
/// rather than guessed, and text with no strong character is left to right.
pub fn first_strong_rtl(text: &str) -> bool {
    text.chars()
        .find_map(|ch| {
            let value = ch as u32;
            match value {
                // Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan, Mandaic and
                // the Arabic presentation forms.
                0x0590..=0x08ff | 0xfb1d..=0xfdff | 0xfe70..=0xfeff | 0x1ee00..=0x1eeff => {
                    Some(true)
                }
                // Latin, Greek, Cyrillic, Armenian, Georgian, Hangul, Han and
                // the Japanese kana.
                0x0041..=0x005a
                | 0x0061..=0x007a
                | 0x00c0..=0x02b8
                | 0x0370..=0x058f
                | 0x10a0..=0x10ff
                | 0x1e00..=0x1fff
                | 0x3040..=0x30ff
                | 0x3400..=0x9fff
                | 0xac00..=0xd7af => Some(false),
                _ => None,
            }
        })
        .unwrap_or(false)
}

/// The distance between baselines for a requested line height.
///
/// All three [`crate::LineHeight`] forms reduce to one target height: a
/// multiple of the natural line, a multiple of the font size, or absolute
/// pixels. Returns `None` when the request or the natural height is not usable,
/// so the platform keeps its own spacing instead of applying a bad value.
pub fn line_height(line_height: crate::LineHeight, natural: f32, font_size: f32) -> Option<f32> {
    if !natural.is_finite() || natural <= 0.0 {
        return None;
    }
    let target = match line_height {
        crate::LineHeight::MetricsRelative(factor) => natural * factor,
        crate::LineHeight::FontSizeRelative(factor) => font_size * factor,
        crate::LineHeight::Absolute(pixels) => pixels,
    };
    (target.is_finite() && target > 0.0).then_some(target)
}

/// The next caret stop one shaping cluster forward or backward.
///
/// `clusters` are the layout's cluster boundaries in logical order, as
/// `(start, end)` UTF-16 code-unit offsets covering the whole text. `unit` is
/// the current caret in code units.
///
/// Movement is logical, not visual, so a right-to-left cluster's next stop is
/// still its logical successor; `nav.rs` documents the same rule for the
/// editor. A caret at a cluster's start steps back to the previous cluster's
/// start, and one at its end steps on to the next cluster's end, so repeated
/// steps always advance instead of sticking. Returns `None` at either end.
pub fn step_cluster(clusters: &[(u32, u32)], unit: u32, forward: bool) -> Option<u32> {
    if clusters.is_empty() {
        return None;
    }
    match forward {
        true => clusters
            .iter()
            .find(|(_, end)| *end > unit)
            .map(|(_, end)| *end),
        false => clusters
            .iter()
            .rev()
            .find(|(start, _)| *start < unit)
            .map(|(start, _)| *start),
    }
}
