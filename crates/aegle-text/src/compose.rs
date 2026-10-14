use std::ops::Range;

use crate::{
    EditorDriver, Selection, TextError,
    editor::{Composition, validate_selection},
    history::Edit,
    nav::Selected,
};

impl EditorDriver<'_> {
    /// Replace the IME preedit without changing the committed value or history.
    /// Cursor endpoints are relative UTF-8 bytes; `None` hides the caret. Empty
    /// preedit cancels, restoring the original value and directed selection.
    pub fn set_preedit(&mut self, text: &str, cursor: Option<Selection>) -> Result<(), TextError> {
        self.composable()?;
        if let Some(cursor) = cursor {
            validate_selection(text, cursor)?;
        }
        let range = self
            .editor
            .composition_range()
            .unwrap_or_else(|| self.editor.selection().range());
        self.validate_replacement(&range, text)?;
        if text.is_empty() {
            self.cancel_preedit();
            return Ok(());
        }
        if self.editor.composition.is_none() {
            self.ready()?;
            self.editor.history.break_group();
            self.editor.composition = Some(Composition {
                original: self.editor.selection(),
                replaced: self.editor.display_text()[range.clone()].to_owned(),
            });
        }
        self.show_preedit(range, text, cursor);
        self.rebuilt(false);
        Ok(())
    }

    /// Replaces `range` with preedit `text` and selects the IME's `cursor`
    /// in it; `None` hides the caret at its start.
    pub(crate) fn show_preedit(
        &mut self,
        range: Range<usize>,
        text: &str,
        cursor: Option<Selection>,
    ) {
        let start = range.start;
        self.splice(range, text, Some(start..start + text.len()));
        self.select_preedit(start, cursor);
    }

    /// Selects the IME's `cursor` in preedit starting at `start`; `None`
    /// hides the caret there.
    pub(crate) fn select_preedit(&mut self, start: usize, cursor: Option<Selection>) {
        let local = cursor.unwrap_or_default();
        let doc = &self.editor.doc;
        let (anchor, focus) = (
            doc.snap(start + local.anchor),
            doc.snap(start + local.focus),
        );
        self.editor.sel = Selected {
            anchor,
            focus,
            ..Selected::collapsed(focus)
        };
        self.editor.show_cursor = cursor.is_some();
    }

    /// Cancel preedit, restoring the replaced fragment and original selection.
    /// Returns whether a composition existed. Suitable for focus loss/destruction
    /// when the platform requests cancellation; it never commits text implicitly.
    pub fn cancel_preedit(&mut self) -> bool {
        let Some(compose) = self.editor.composition.take() else {
            return false;
        };
        let range = self.editor.composition_range().expect("composition range");
        self.splice(range, &compose.replaced, None);
        self.editor.show_cursor = true;
        self.select_snapped(compose.original);
        self.rebuilt(false);
        true
    }

    /// Commit an IME result as one undo group, replacing preedit or the selection.
    /// An empty commit deletes that range; it is distinct from cancelling preedit.
    pub fn commit(&mut self, text: &str) -> Result<(), TextError> {
        self.writable()?;
        let range = self
            .editor
            .composition_range()
            .unwrap_or_else(|| self.editor.selection().range());
        self.validate_replacement(&range, text)?;
        if self.editor.composition.is_none() {
            self.ready()?;
        }
        self.editor.history.break_group();
        let Some(compose) = self.editor.composition.take() else {
            self.replace_selected(text, false);
            return Ok(());
        };
        let changed = compose.replaced != text;
        self.splice(range.clone(), text, None);
        self.place_after(range.start, text);
        if changed
            && self
                .editor
                .history
                .prepare(compose.replaced.len(), text.len())
        {
            self.editor.history.record(
                Edit {
                    start: range.start,
                    removed: compose.replaced,
                    inserted: text.to_owned(),
                    before: compose.original,
                    after: self.editor.visual_selection(),
                },
                false,
            );
        }
        self.rebuilt(changed);
        Ok(())
    }

    /// Replace an exact UTF-8 byte range as one edit. Unlike user selection this
    /// does not snap endpoints to shaping clusters, so native protocol adapters
    /// and undo can remove individual combining characters without deleting a base.
    /// Active preedit must be handled explicitly before an external replacement.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), TextError> {
        self.writable()?;
        self.no_composition()?;
        if range.start > range.end {
            return Err(TextError::InvalidRange);
        }
        validate_selection(
            self.editor.display_text(),
            Selection {
                anchor: range.start,
                focus: range.end,
            },
        )?;
        self.validate_replacement(&range, text)?;
        self.editor.history.break_group();
        self.replace_range(range, text);
        Ok(())
    }
}
