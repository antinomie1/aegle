use std::fmt;

use crate::{Editor, Selection};

/// Bounded surrounding text for an input method, excluding transient preedit.
///
/// The two borrowed pieces concatenate without allocation. Convert with
/// `to_string()` only when a native API needs an owned contiguous string.
#[derive(Clone, Copy, Debug)]
pub struct Surrounding<'a> {
    parts: [&'a str; 2],
    /// Directed UTF-8 selection relative to this excerpt. During composition it
    /// collapses where the excluded preedit was; the original selection is absent.
    pub selection: Selection,
}

impl<'a> Surrounding<'a> {
    /// Ordered, possibly empty UTF-8 pieces.
    pub fn parts(self) -> [&'a str; 2] {
        self.parts
    }
    /// Total UTF-8 bytes in the excerpt.
    pub fn len(self) -> usize {
        self.parts[0].len() + self.parts[1].len()
    }
    /// Whether the excerpt contains no text.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for Surrounding<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.parts[0])?;
        f.write_str(self.parts[1])
    }
}

impl Editor {
    /// Borrow at most `max_bytes` of surrounding text, including the complete
    /// selection and roughly balanced context. Returns `None` when the selection
    /// alone exceeds the budget. All slice boundaries remain valid UTF-8.
    ///
    /// During composition, both preedit and its original replaced selection are
    /// excluded, unlike [`Editor::text`]. Native restrictions such as NUL bytes
    /// or protocol-specific length limits remain the adapter's responsibility.
    pub fn surrounding(&self, max_bytes: usize) -> Option<Surrounding<'_>> {
        let display = self.display_text();
        let (prefix, suffix, selection) = match self.composition_range() {
            Some(range) => (
                &display[..range.start],
                &display[range.end..],
                Selection {
                    anchor: range.start,
                    focus: range.start,
                },
            ),
            None => (display, "", self.selection()),
        };
        let selected = selection.range();
        let spare = max_bytes.checked_sub(selected.len())?;
        let length = prefix.len() + suffix.len();
        let mut start = selected.start.saturating_sub(spare / 2);
        let mut end = start.saturating_add(max_bytes).min(length);
        start = end.saturating_sub(max_bytes).min(start);
        let boundary = |index| {
            if index <= prefix.len() {
                prefix.is_char_boundary(index)
            } else {
                suffix.is_char_boundary(index - prefix.len())
            }
        };
        while !boundary(start) {
            start += 1;
        }
        while !boundary(end) {
            end -= 1;
        }
        let first = if start < prefix.len() {
            &prefix[start..end.min(prefix.len())]
        } else {
            ""
        };
        let second = if end > prefix.len() {
            &suffix[start.saturating_sub(prefix.len())..end - prefix.len()]
        } else {
            ""
        };
        Some(Surrounding {
            parts: [first, second],
            selection: Selection {
                anchor: selection.anchor - start,
                focus: selection.focus - start,
            },
        })
    }
}
