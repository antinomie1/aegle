use aegle_types::{Point, Size};
use std::ops::Range;

use crate::{
    Alignment, Editor, Selection, TextError, TextStyle, TextSystem,
    document::Caret,
    editor::{validate_content, validate_selection},
    history::Edit,
    layout::Affinity,
    nav::{Selected, Unit},
};

/// Direction or destination for keyboard/assistive cursor motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    /// Previous visual cluster.
    Left,
    /// Next visual cluster.
    Right,
    /// Previous visual word boundary.
    WordLeft,
    /// Next visual word boundary.
    WordRight,
    /// Previous display line, preserving horizontal intent.
    Up,
    /// Next display line, preserving horizontal intent.
    Down,
    /// Start of the display line.
    LineStart,
    /// End of the display line.
    LineEnd,
    /// Start of the complete text.
    TextStart,
    /// End of the complete text.
    TextEnd,
}

/// Point-selection granularity for clicks, dragging and touch selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitSelection {
    /// Collapse at the nearest visual cluster edge.
    Caret,
    /// Extend the existing selection, preserving word/line granularity.
    Extend,
    /// Select the containing word.
    Word,
    /// Select the containing display line.
    Line,
}

/// Short-lived imperative editing access with shared font/shaping contexts.
pub struct EditorDriver<'a> {
    pub(crate) editor: &'a mut Editor,
    pub(crate) system: &'a mut TextSystem,
}

impl EditorDriver<'_> {
    /// Replace styling without replacing the editor, selection or preedit.
    /// A new color alone shapes nothing; other changes shape every line.
    pub fn restyle(&mut self, style: &TextStyle<'_>) -> Result<(), TextError> {
        style.validate()?;
        let doc = &mut self.editor.doc;
        doc.color = style.color;
        if doc.shaping.matches(style) {
            self.editor.changes.paint = true;
            return Ok(());
        }
        doc.shaping = self.system.shaping(style);
        doc.reshape(self.system);
        self.rebuilt(false);
        Ok(())
    }

    /// Reflow within a finite nonnegative width; `None` disables soft wrapping.
    /// Single-line editors keep soft wrapping disabled. Identical constraints
    /// do no work, and others only break and align lines again.
    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) -> Result<Size, TextError> {
        if width.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err(TextError::InvalidWidth);
        }
        let doc = &mut self.editor.doc;
        if (doc.width, doc.alignment) != (width, alignment) {
            doc.reflow(width, alignment);
            self.rebuilt(false);
        }
        Ok(self.editor.size())
    }

    /// Replace the selected value. Adjacent pure insertions coalesce for undo;
    /// call [`Editor::break_undo_group`] around paste or another distinct action.
    pub fn insert(&mut self, text: &str) -> Result<(), TextError> {
        self.writable()?;
        self.no_composition()?;
        self.ready()?;
        let range = self.editor.selection().range();
        self.validate_replacement(&range, text)?;
        self.replace_selected(text, true);
        Ok(())
    }

    /// Replace the complete value programmatically and clear undo history.
    /// Read-only affects user edits, not this explicit application setter.
    pub fn set_text(&mut self, text: &str) -> Result<(), TextError> {
        validate_content(text, self.editor.multiline)?;
        let changed = self.editor.text() != text;
        self.cancel_preedit();
        if changed {
            let all = 0..self.editor.doc.text.len();
            if let Some(secret) = &mut self.editor.secret {
                secret.replace_range(.., text);
                self.splice(all, &mask(text), None);
            } else {
                self.splice(all, text, None);
            }
            self.select_end();
            self.rebuilt(true);
        }
        self.editor.history.clear();
        Ok(())
    }

    /// Display one mask per character and keep the value outside the layout.
    /// Toggling clears undo history and preedit; password editors keep no
    /// history and reject IME transactions with [`TextError::Password`].
    pub fn set_password(&mut self, password: bool) {
        if self.editor.is_password() == password {
            return;
        }
        self.cancel_preedit();
        let all = 0..self.editor.doc.text.len();
        if password {
            let value = self.editor.text().to_string();
            self.splice(all, &mask(&value), None);
            self.editor.secret = Some(value);
        } else {
            let value = self.editor.secret.take().unwrap();
            self.splice(all, &value, None);
        }
        self.select_end();
        self.rebuilt(false);
        self.editor.history.clear();
        self.editor.changes.policy = true;
    }

    /// Delete the selection, or the preceding Unicode extended grapheme.
    pub fn backspace(&mut self) -> Result<(), TextError> {
        self.delete_cluster(true)
    }
    /// Delete the selection, or the following Unicode extended grapheme.
    pub fn delete(&mut self) -> Result<(), TextError> {
        self.delete_cluster(false)
    }

    fn delete_cluster(&mut self, previous: bool) -> Result<(), TextError> {
        self.writable()?;
        self.no_composition()?;
        self.ready()?;
        let mut range = self.editor.selection().range();
        if range.is_empty() {
            range = self.grapheme(range.start, previous);
        }
        self.editor.history.break_group();
        if !range.is_empty() {
            self.replace_range(range, "");
        }
        Ok(())
    }

    /// The extended grapheme before or after `caret`. Graphemes never span a
    /// hard line break other than CRLF, so this segments one line.
    fn grapheme(&self, caret: usize, previous: bool) -> Range<usize> {
        let doc = &self.editor.doc;
        let block = &doc.blocks[doc.block_at(caret)];
        let edge = match previous {
            true => caret == block.range.start,
            false => caret >= block.range.end,
        };
        if edge {
            // The break itself: before this line, or after it.
            let index = doc.block_at(caret);
            return match previous {
                true if index > 0 => doc.blocks[index - 1].range.end..block.range.start,
                false if index + 1 < doc.blocks.len() => {
                    block.range.end..doc.blocks[index + 1].range.start
                }
                _ => caret..caret,
            };
        }
        let line = &doc.text[block.range.clone()];
        let caret = caret - block.range.start;
        let range = block.layout.grapheme_range(line, caret, previous);
        block.range.start + range.start..block.range.start + range.end
    }

    /// Select UTF-8 byte endpoints. Endpoints inside shaping clusters snap to the
    /// cluster start; this method is for user selection, not raw protocol edits.
    pub fn select(&mut self, selection: Selection) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        validate_selection(self.editor.display_text(), selection)?;
        self.select_snapped(selection);
        Ok(())
    }

    /// Move in visual order, optionally extending from the existing anchor.
    pub fn move_cursor(&mut self, movement: Movement, extend: bool) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        let (doc, sel) = (&self.editor.doc, self.editor.sel);
        let moved = match movement {
            Movement::Left => doc.horizontal(sel, false, false, extend),
            Movement::Right => doc.horizontal(sel, true, false, extend),
            Movement::WordLeft => doc.horizontal(sel, false, true, extend),
            Movement::WordRight => doc.horizontal(sel, true, true, extend),
            Movement::Up => doc.vertical(sel, false, extend),
            Movement::Down => doc.vertical(sel, true, extend),
            Movement::LineStart => doc.line_edge(sel, false, extend),
            Movement::LineEnd => doc.line_edge(sel, true, extend),
            Movement::TextStart => doc.text_edge(sel, false, extend),
            Movement::TextEnd => doc.text_edge(sel, true, extend),
        };
        self.moved(moved);
        Ok(())
    }

    /// Hit-test a point in the same local coordinates used to paint the layout.
    pub fn select_at(&mut self, point: Point, how: HitSelection) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(TextError::InvalidPosition);
        }
        let (doc, sel) = (&self.editor.doc, self.editor.sel);
        let moved = match how {
            HitSelection::Caret => Selected::collapsed(doc.hit(point)),
            HitSelection::Extend => doc.extend_to(sel, point),
            HitSelection::Word => doc.unit_at(point, Unit::Word),
            HitSelection::Line => doc.unit_at(point, Unit::Line),
        };
        self.moved(moved);
        Ok(())
    }

    /// Disable user edits, cancelling preedit while preserving the committed value.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.editor.changes.policy |= self.editor.read_only != read_only;
        if read_only {
            self.cancel_preedit();
        }
        self.editor.read_only = read_only;
        self.editor.history.break_group();
    }

    /// Undo one complete edit group. Active preedit must be cancelled/committed first.
    pub fn undo(&mut self) -> Result<bool, TextError> {
        self.travel_history(false)
    }
    /// Redo one complete edit group.
    pub fn redo(&mut self) -> Result<bool, TextError> {
        self.travel_history(true)
    }

    fn travel_history(&mut self, redo: bool) -> Result<bool, TextError> {
        self.writable()?;
        self.no_composition()?;
        let editor = &mut *self.editor;
        let entry = if redo {
            editor.history.redo()
        } else {
            editor.history.undo()
        };
        let Some(entry) = entry else {
            return Ok(false);
        };
        let (range, text, selection) = if redo {
            let range = entry.start..entry.start + entry.removed.len();
            (range, entry.inserted.as_str(), entry.after)
        } else {
            let range = entry.start..entry.start + entry.inserted.len();
            (range, entry.removed.as_str(), entry.before)
        };
        editor.doc.splice(self.system, range, text, None);
        self.select_snapped(selection);
        self.rebuilt(true);
        Ok(true)
    }

    pub(crate) fn writable(&self) -> Result<(), TextError> {
        if self.editor.read_only {
            Err(TextError::ReadOnly)
        } else {
            Ok(())
        }
    }
    pub(crate) fn no_composition(&self) -> Result<(), TextError> {
        if self.editor.composition.is_some() {
            Err(TextError::CompositionActive)
        } else {
            Ok(())
        }
    }
    pub(crate) fn composable(&self) -> Result<(), TextError> {
        self.writable()?;
        if self.editor.is_password() {
            Err(TextError::Password)
        } else {
            Ok(())
        }
    }
    pub(crate) fn ready(&self) -> Result<(), TextError> {
        if self.editor.doc.diagnostics.unshaped_bytes != 0 {
            Err(TextError::MissingFont)
        } else {
            Ok(())
        }
    }
    pub(crate) fn validate_replacement(
        &self,
        range: &Range<usize>,
        text: &str,
    ) -> Result<(), TextError> {
        validate_content(text, self.editor.multiline)?;
        if (self.editor.display_text().len() - range.len())
            .checked_add(text.len())
            .is_none_or(|n| n > u32::MAX as usize)
        {
            return Err(TextError::TextTooLong);
        }
        Ok(())
    }
    pub(crate) fn replace_selected(&mut self, text: &str, merge: bool) {
        let before = self.editor.selection();
        let range = before.range();
        let masked = self.conceal(&range, text);
        let text = masked.as_deref().unwrap_or(text);
        if masked.is_none() && self.editor.display_text()[range.clone()] == *text {
            if !range.is_empty() {
                self.moved(Selected::collapsed(self.editor.doc.snap(range.end)));
            }
            return;
        }
        let removed = self.capture(&range, text);
        self.splice(range.clone(), text, None);
        self.place_after(range.start, text);
        self.record(range.start, removed, text, before, merge);
        self.rebuilt(true);
    }
    pub(crate) fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let before = self.editor.selection();
        let masked = self.conceal(&range, text);
        let text = masked.as_deref().unwrap_or(text);
        if masked.is_none() && self.editor.display_text()[range.clone()] == *text {
            return;
        }
        let removed = self.capture(&range, text);
        self.splice(range.clone(), text, None);
        self.place_after(range.start, text);
        self.record(range.start, removed, text, before, false);
        self.rebuilt(true);
    }
    /// Replaces display bytes, shaping only the lines involved.
    pub(crate) fn splice(
        &mut self,
        range: Range<usize>,
        text: &str,
        compose: Option<Range<usize>>,
    ) {
        self.editor.doc.splice(self.system, range, text, compose);
    }
    /// Collapses the selection after `text` inserted at `start`.
    pub(crate) fn place_after(&mut self, start: usize, text: &str) {
        let index = start + text.len();
        // After a line break the caret starts the next line.
        let affinity = match text.ends_with(['\n', '\r', '\u{2028}', '\u{2029}']) {
            true => Affinity::Downstream,
            false => Affinity::Upstream,
        };
        self.editor.sel = Selected::collapsed(Caret { index, affinity });
        self.editor.show_cursor = true;
    }
    /// Selects byte endpoints snapped to clusters, without clearing history coalescing.
    pub(crate) fn select_snapped(&mut self, selection: Selection) {
        let doc = &self.editor.doc;
        let (anchor, focus) = (doc.snap(selection.anchor), doc.snap(selection.focus));
        self.moved(Selected {
            anchor,
            focus,
            ..Selected::collapsed(focus)
        });
    }
    fn select_end(&mut self) {
        let end = Caret {
            index: self.editor.doc.text.len(),
            affinity: Affinity::Upstream,
        };
        self.editor.sel = Selected::collapsed(end);
        self.editor.show_cursor = true;
    }
    fn capture(&mut self, range: &Range<usize>, text: &str) -> Option<String> {
        (!self.editor.is_password() && self.editor.history.prepare(range.len(), text.len()))
            .then(|| self.editor.display_text()[range.clone()].to_owned())
    }
    /// Mirrors a display edit into the password value, returning its masks.
    fn conceal(&mut self, range: &Range<usize>, text: &str) -> Option<String> {
        let secret = self.editor.secret.as_mut()?;
        let byte = |index: usize| {
            secret
                .char_indices()
                .nth(index / MASK.len_utf8())
                .map_or(secret.len(), |(byte, _)| byte)
        };
        let range = byte(range.start)..byte(range.end);
        secret.replace_range(range, text);
        Some(mask(text))
    }
    fn record(
        &mut self,
        start: usize,
        removed: Option<String>,
        text: &str,
        before: Selection,
        merge: bool,
    ) {
        if let Some(removed) = removed {
            self.editor.history.record(
                Edit {
                    start,
                    removed,
                    inserted: text.to_owned(),
                    before,
                    after: self.editor.visual_selection(),
                },
                merge,
            );
        }
    }
    pub(crate) fn rebuilt(&mut self, value: bool) {
        #[cfg(feature = "text-a11y")]
        self.editor.access_runs.clear();
        self.editor.changes.value |= value;
        self.editor.revision += u64::from(value);
        self.editor.changes.layout = true;
        self.editor.changes.selection = true;
    }
    /// Takes a new selection made by a user motion.
    pub(crate) fn moved(&mut self, selection: Selected) {
        self.editor.history.break_group();
        self.editor.changes.selection |= !selection.same(&self.editor.sel);
        self.editor.sel = selection;
    }
}

const MASK: char = '\u{2022}';

fn mask(text: &str) -> String {
    std::iter::repeat_n(MASK, text.chars().count()).collect()
}
