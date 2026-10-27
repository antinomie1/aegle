//! The input-method text model of one focused editor.
//!
//! macOS describes an edit as an operation (`insertText:`, `setMarkedText:`)
//! rather than as a changed document, so each operation maps directly to one
//! [`ImeUpdate`] and no document diffing is needed. [`Composition`] holds the
//! editor's surrounding excerpt as the host last reported it and answers the
//! `NSTextInputClient` queries, whose ranges count UTF-16 units, while
//! [`ImeUpdate`] offsets count UTF-8 bytes.
//!
//! Ranges address the document the system sees: the excerpt with the
//! composition standing in for the selection.
use crate::ime_text::{to_byte, to_range, to_units, units};
use aegle_platform::{ImeUpdate, Preedit};
use std::{ops::Range, string::String};

/// A composition in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marked {
    /// Temporary text, not yet committed to the document.
    pub text: String,
    /// Fixed and active endpoints within `text`, in UTF-8 bytes.
    pub cursor: Option<(usize, usize)>,
}

/// What one key press taught the input client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyOutcome {
    /// The input method edited or ended the composition, so the key is not a
    /// shortcut.
    pub consumed: bool,
    /// Text the key typed directly, without composing it.
    pub text: Option<String>,
}

/// One editor's session: what the host reported, plus the composition.
#[derive(Debug, Default)]
pub struct Composition {
    focused: bool,
    surrounding: String,
    anchor: usize,
    cursor: usize,
    marked: Option<Marked>,
    key: Option<KeyOutcome>,
}

impl Composition {
    /// A session that no editor has claimed yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables the session, or updates it for the same editor.
    ///
    /// `surrounding` is the committed text around the selection with
    /// `anchor` and `cursor` as UTF-8 byte offsets into it, as the contract's
    /// [`aegle_platform::ImeRequest`] carries them. Returns whether the
    /// session was enabled, so the caller can report
    /// [`aegle_platform::ImeEvent::Entered`] once.
    pub fn focus(&mut self, surrounding: &str, anchor: usize, cursor: usize) -> bool {
        self.surrounding = surrounding.to_owned();
        self.anchor = anchor;
        self.cursor = cursor;
        !std::mem::replace(&mut self.focused, true)
    }

    /// Ends the session without committing its composition, which belongs to
    /// the old editor. Returns whether a composition was discarded.
    pub fn blur(&mut self) -> bool {
        self.focused = false;
        self.key = None;
        self.marked.take().is_some()
    }

    /// Whether an editor currently owns the session.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Whether an input method is composing.
    pub fn is_composing(&self) -> bool {
        self.marked.is_some()
    }

    /// The committed excerpt around the selection.
    pub fn surrounding(&self) -> &str {
        &self.surrounding
    }

    /// The selection as bytes of [`Self::surrounding`], ordered.
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    /// The document the system addresses: the excerpt with the composition
    /// standing in for the selection.
    pub fn document(&self) -> String {
        let Some(marked) = &self.marked else {
            return self.surrounding.clone();
        };
        let selection = self.selection();
        let mut text = String::with_capacity(self.surrounding.len() + marked.text.len());
        text.push_str(&self.surrounding[..selection.start]);
        text.push_str(&marked.text);
        text.push_str(&self.surrounding[selection.end..]);
        text
    }

    /// The composition's UTF-16 range in the document, if there is one.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        let marked = self.marked.as_ref()?;
        let start = to_units(&self.surrounding, self.selection().start);
        Some(start..start + units(&marked.text))
    }

    /// The selection's UTF-16 range in the document: the composition's own
    /// selection while composing.
    pub fn selected_range(&self) -> Range<usize> {
        if let Some(marked) = &self.marked {
            let start = to_units(&self.surrounding, self.selection().start);
            let (anchor, focus) = marked
                .cursor
                .unwrap_or((marked.text.len(), marked.text.len()));
            let range = to_range(&marked.text, anchor.min(focus)..anchor.max(focus));
            return start + range.start..start + range.end;
        }
        to_range(&self.surrounding, self.selection())
    }

    /// The document text of a UTF-16 `range` and the range actually covered;
    /// `None` for a range beyond the document.
    pub fn substring(&self, range: Range<usize>) -> Option<(String, Range<usize>)> {
        let document = self.document();
        let bytes = byte_range(&document, range)?;
        let actual = to_range(&document, bytes.clone());
        Some((document[bytes].to_owned(), actual))
    }

    /// Starts recording what one key press does to the session.
    pub fn key_down(&mut self) {
        self.key = Some(KeyOutcome::default());
    }

    /// Ends the key press and reports what the input client did.
    pub fn key_up(&mut self) -> KeyOutcome {
        self.key.take().unwrap_or_default()
    }

    /// Applies `setMarkedText:selectedRange:replacementRange:`: show `text`
    /// as the composition with `selected` as its UTF-16 selection, replacing
    /// the UTF-16 `replacement` range or, when `None`, the current selection.
    /// Returns the update for the host, or `None` when no editor is focused.
    pub fn set_marked(
        &mut self,
        text: &str,
        selected: Range<usize>,
        replacement: Option<Range<usize>>,
    ) -> Option<ImeUpdate> {
        if !self.focused {
            return None;
        }
        let restart = self.marked.is_none();
        let (delete_before, delete_after) = self.replacement(replacement);
        let cursor = byte_offsets(text, selected);
        let ended = text.is_empty();
        if let Some(key) = &mut self.key {
            key.consumed = true;
        }
        self.marked = (!ended).then(|| Marked {
            text: text.to_owned(),
            cursor,
        });
        Some(ImeUpdate {
            delete_before,
            delete_after,
            // Starting over a selection replaces it; a running composition is
            // replaced by its successor.
            commit: restart.then(String::new),
            preedit: Preedit {
                text: text.to_owned(),
                cursor: cursor.filter(|_| !ended),
            },
        })
    }

    /// Applies `insertText:replacementRange:`: commit `text` in place of the
    /// UTF-16 `replacement` range or, when `None`, the current selection.
    ///
    /// Text a key press types outside any composition is not an edit but the
    /// key's own text, which [`Self::key_up`] then reports. Returns `None`
    /// when no editor is focused.
    pub fn insert(&mut self, text: &str, replacement: Option<Range<usize>>) -> Option<ImeUpdate> {
        if !self.focused {
            return None;
        }
        let (delete_before, delete_after) = self.replacement(replacement);
        let composing = self.marked.is_some();
        if let Some(key) = &mut self.key
            && !composing
            && !key.consumed
            && delete_before + delete_after == 0
        {
            key.text = Some(text.to_owned());
            return None;
        }
        if let Some(key) = &mut self.key {
            key.consumed = true;
        }
        self.marked = None;
        let commit = self.commit(delete_before, delete_after, text);
        Some(commit)
    }

    /// Applies `unmarkText`: the composition becomes committed text as it
    /// stands, which is what an input method that gives up on its composition
    /// expects.
    pub fn unmark(&mut self) -> Option<ImeUpdate> {
        if let Some(key) = &mut self.key {
            key.consumed = true;
        }
        let marked = self.marked.take()?;
        Some(self.commit(0, 0, &marked.text))
    }

    /// The bytes around the selection that a UTF-16 `replacement` covers, as
    /// `(delete_before, delete_after)`; a replacement that does not enclose
    /// the selection cannot be expressed and deletes nothing.
    fn replacement(&self, replacement: Option<Range<usize>>) -> (usize, usize) {
        if self.marked.is_some() {
            // While composing, the range addresses the preedit, which the
            // update itself replaces.
            return (0, 0);
        }
        let Some(covered) = replacement.and_then(|range| byte_range(&self.surrounding, range))
        else {
            return (0, 0);
        };
        let selection = self.selection();
        if covered.start <= selection.start && covered.end >= selection.end {
            (selection.start - covered.start, covered.end - selection.end)
        } else {
            (0, 0)
        }
    }

    /// Replaces the selection plus `delete_before`/`delete_after` bytes with
    /// `text`, so later queries within the same key press see the result
    /// until the host reports its own state again.
    fn commit(&mut self, delete_before: usize, delete_after: usize, text: &str) -> ImeUpdate {
        let selection = self.selection();
        let start = selection.start - delete_before;
        let end = selection.end + delete_after;
        self.surrounding.replace_range(start..end, text);
        self.anchor = start + text.len();
        self.cursor = self.anchor;
        ImeUpdate {
            delete_before,
            delete_after,
            commit: Some(text.to_owned()),
            preedit: Preedit::default(),
        }
    }
}

/// The UTF-8 byte range of a UTF-16 range of `text`, clamped to it.
fn byte_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let start = to_byte(text, range.start);
    let end = to_byte(text, range.end).max(start);
    Some(start..end)
}

/// The UTF-8 byte offsets of a UTF-16 range inside `text`.
fn byte_offsets(text: &str, range: Range<usize>) -> Option<(usize, usize)> {
    let start = to_byte(text, range.start);
    Some((start, to_byte(text, range.end).max(start)))
}

#[cfg(test)]
mod tests {
    // Unit conversions are covered by the crate's integration tests, which
    // exercise them through `Composition`'s public API.
}
