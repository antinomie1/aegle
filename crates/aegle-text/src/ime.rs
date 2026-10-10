use std::{borrow::Cow, ops::Range};

use crate::{
    Editor, EditorDriver, Selection, TextError,
    edit::raw_replace,
    editor::{Composition, validate_content, validate_selection},
    history::Edit,
};

/// One platform-neutral input-method transaction, using UTF-8 byte offsets.
///
/// Native adapters convert their offset units before submitting this batch.
/// Empty preedit removes an old composition to a caret; explicit application
/// cancellation instead uses [`EditorDriver::cancel_preedit`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ImeEdit<'a> {
    /// Bytes to delete before the selection or preedit, excluding that range.
    pub delete_before: usize,
    /// Bytes to delete after the selection or preedit, excluding that range.
    pub delete_after: usize,
    /// Committed replacement. `None` differs from `Some("")`, which deletes.
    pub commit: Option<&'a str>,
    /// New preedit following deletion and commit; empty means no preedit.
    pub preedit: &'a str,
    /// Directed UTF-8 offsets relative to `preedit`; `None` hides the caret.
    pub cursor: Option<Selection>,
}

impl EditorDriver<'_> {
    /// Validate and apply a complete native IME batch without partial mutation
    /// on error. Surrounding deletion and commit form one undo operation.
    ///
    /// Preedit-only updates retain the original selection for cancellation and
    /// eventual commit. Surrounding deletions remain committed edits even while
    /// preedit is active. History copies only affected fragments within its budget.
    pub fn apply_ime(&mut self, edit: ImeEdit<'_>) -> Result<(), TextError> {
        self.composable()?;
        if self.editor.composition.is_none() {
            self.ready()?;
        }
        let (range, commit) = self.editor.ime_plan(edit)?;
        let before = self.editor.selection();
        let selected = self
            .editor
            .composition_range()
            .unwrap_or_else(|| before.range());
        if let Some(commit) = commit {
            self.ime_commit(range, selected, before, commit, edit);
        } else {
            self.ime_preedit(range, selected, before, edit);
        }
        Ok(())
    }

    fn ime_commit(
        &mut self,
        range: Range<usize>,
        selected: Range<usize>,
        before: Selection,
        text: &str,
        edit: ImeEdit<'_>,
    ) {
        let parts = self.ime_removed(&range, &selected);
        let changed = !parts.into_iter().flat_map(str::bytes).eq(text.bytes());
        let removed = changed
            .then(|| self.ime_capture(&range, &selected, text.len()))
            .flatten();
        self.editor.history.break_group();
        self.editor.composition = None;
        let visible = if edit.preedit.is_empty() {
            Cow::Borrowed(text)
        } else if text.is_empty() {
            Cow::Borrowed(edit.preedit)
        } else {
            let mut visible = String::with_capacity(text.len() + edit.preedit.len());
            visible.push_str(text);
            visible.push_str(edit.preedit);
            Cow::Owned(visible)
        };
        raw_replace(&mut self.engine(), range.clone(), &visible);
        let cursor = range.start + text.len();
        let after = Selection {
            anchor: cursor,
            focus: cursor,
        };
        if !edit.preedit.is_empty() {
            self.editor.composition = Some(Composition {
                original: after,
                replaced: String::new(),
            });
            let mut engine = self.engine();
            // Never mark an empty composing range: Parley would generate an
            // empty style run. The combined insertion also preserves exact byte
            // positions when the commit endpoint lies inside a shaping cluster.
            engine.set_compose_byte_range(cursor, cursor + edit.preedit.len());
            engine.set_compose(edit.preedit, edit.cursor.map(|s| (s.anchor, s.focus)));
        }
        if let Some(removed) = removed {
            self.editor.history.record(
                Edit {
                    start: range.start,
                    removed,
                    inserted: text.to_owned(),
                    before,
                    after,
                },
                false,
            );
        }
        self.rebuilt(changed);
    }

    fn ime_preedit(
        &mut self,
        range: Range<usize>,
        selected: Range<usize>,
        before: Selection,
        edit: ImeEdit<'_>,
    ) {
        let deleted = edit.delete_before != 0 || edit.delete_after != 0;
        let removed = deleted
            .then(|| self.ime_capture(&range, &selected, self.editor.selected_text().len()))
            .flatten();
        let inserted = removed
            .as_ref()
            .map(|_| self.editor.selected_text().to_owned());
        let after = Selection {
            anchor: before.anchor - edit.delete_before,
            focus: before.focus - edit.delete_before,
        };
        if !edit.preedit.is_empty() {
            if let Some(composition) = &mut self.editor.composition {
                composition.original = after;
            } else {
                self.editor.history.break_group();
                self.editor.composition = Some(Composition {
                    original: after,
                    replaced: self.editor.selected_text().to_owned(),
                });
            }
            let mut engine = self.engine();
            if deleted {
                engine.set_compose_byte_range(range.start, range.end);
            }
            engine.set_compose(edit.preedit, edit.cursor.map(|s| (s.anchor, s.focus)));
            self.rebuilt(deleted);
        } else if deleted {
            // No composition and no commit: delete the two sides while retaining
            // the selected fragment without allocating a replacement copy.
            let mut engine = self.engine();
            if edit.delete_after != 0 {
                raw_replace(&mut engine, selected.end..range.end, "");
            }
            if edit.delete_before != 0 {
                raw_replace(&mut engine, range.start..selected.start, "");
            }
            engine.select_byte_range(after.anchor, after.focus);
            self.rebuilt(true);
        }
        if let (Some(removed), Some(inserted)) = (removed, inserted) {
            self.editor.history.record(
                Edit {
                    start: range.start,
                    removed,
                    inserted,
                    before,
                    after,
                },
                false,
            );
        }
    }

    fn ime_removed<'a>(&'a self, range: &Range<usize>, selected: &Range<usize>) -> [&'a str; 3] {
        let text = self.editor.display_text();
        [
            &text[range.start..selected.start],
            self.editor.selected_text(),
            &text[selected.end..range.end],
        ]
    }

    fn ime_capture(
        &mut self,
        range: &Range<usize>,
        selected: &Range<usize>,
        inserted: usize,
    ) -> Option<String> {
        let length = range.len() - selected.len() + self.editor.selected_text().len();
        if !self.editor.history.prepare(length, inserted) {
            return None;
        }
        let mut removed = String::with_capacity(length);
        for part in self.ime_removed(range, selected) {
            removed.push_str(part);
        }
        Some(removed)
    }
}

impl Editor {
    /// Checks a native IME batch against this editor without applying it:
    /// [`EditorDriver::apply_ime`] then succeeds on an editable, non-password
    /// editor with a font. Hosts validate platform input with it before
    /// routing the batch to a control.
    pub fn check_ime(&self, edit: ImeEdit<'_>) -> Result<(), TextError> {
        self.ime_plan(edit).map(drop)
    }

    /// The replaced range and committed text of a valid batch.
    fn ime_plan<'a>(
        &self,
        edit: ImeEdit<'a>,
    ) -> Result<(Range<usize>, Option<&'a str>), TextError> {
        validate_content(edit.preedit, self.multiline)?;
        if let Some(cursor) = edit.cursor {
            validate_selection(edit.preedit, cursor)?;
        }
        if let Some(commit) = edit.commit {
            validate_content(commit, self.multiline)?;
        }
        let selected = self
            .composition_range()
            .unwrap_or_else(|| self.selection().range());
        let start = selected
            .start
            .checked_sub(edit.delete_before)
            .ok_or(TextError::InvalidRange)?;
        let end = selected
            .end
            .checked_add(edit.delete_after)
            .ok_or(TextError::InvalidRange)?;
        validate_selection(
            self.display_text(),
            Selection {
                anchor: start,
                focus: end,
            },
        )?;
        let range = start..end;
        let commit = edit
            .commit
            .or_else(|| (self.composition.is_some() && edit.preedit.is_empty()).then_some(""));
        // Check committed and displayed lengths separately: a composition can
        // retain a much larger selected fragment than the visible preedit.
        let remaining = self.display_text().len() - range.len();
        let inserted = commit.map_or(self.selected_text().len(), str::len);
        checked_length(remaining, inserted)?;
        let displayed = if edit.preedit.is_empty() {
            inserted
        } else {
            edit.preedit
                .len()
                .checked_add(commit.map_or(0, str::len))
                .ok_or(TextError::TextTooLong)?
        };
        checked_length(remaining, displayed)?;
        Ok((range, commit))
    }
}

fn checked_length(remaining: usize, inserted: usize) -> Result<(), TextError> {
    if remaining
        .checked_add(inserted)
        .is_none_or(|n| n > u32::MAX as usize)
    {
        Err(TextError::TextTooLong)
    } else {
        Ok(())
    }
}
