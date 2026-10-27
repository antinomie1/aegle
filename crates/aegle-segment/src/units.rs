//! UTF-16 offsets, which the system segmenters count, to UTF-8 bytes.

/// The UTF-8 byte offset of each UTF-16 `offset` in `text`; an offset inside a
/// surrogate pair maps to the byte after its character, and offsets past the
/// text map to its length. Out-of-range results are filtered by
/// [`crate::Boundaries`], which keeps only character boundaries.
pub(crate) fn to_bytes(text: &str, offsets: &[usize]) -> Vec<usize> {
    let mut bytes = Vec::with_capacity(offsets.len());
    let mut next = 0;
    let mut unit = 0;
    for (byte, ch) in text.char_indices() {
        while offsets.get(next).is_some_and(|&offset| offset <= unit) {
            bytes.push(byte);
            next += 1;
        }
        unit += ch.len_utf16();
    }
    while next < offsets.len() {
        bytes.push(text.len());
        next += 1;
    }
    bytes
}
