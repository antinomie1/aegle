use aegle_types::{Color, Point, Size};
use parley::{PlainEditorDriver, StyleProperty, style::FontFamily};
use std::{borrow::Cow, ops::Range};

use crate::{
    Alignment, Editor, Selection, TextError, TextStyle, TextSystem,
    editor::{validate_content, validate_selection},
    history::Edit,
    paragraph::diagnose,
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
    pub(crate) fn engine(&mut self) -> PlainEditorDriver<'_, Color> {
        self.editor
            .inner
            .driver(&mut self.system.fonts, &mut self.system.context)
    }

    /// Replace styling without replacing the editor, selection or preedit.
    /// Foreground-only drawing overrides can instead avoid reshaping entirely.
    pub fn restyle(&mut self, style: &TextStyle<'_>) -> Result<(), TextError> {
        style.validate()?;
        self.apply_style(style);
        Ok(())
    }

    pub(crate) fn apply_style(&mut self, style: &TextStyle<'_>) {
        let styles = self.editor.inner.edit_styles();
        styles.insert(StyleProperty::FontFamily(FontFamily::Source(Cow::Owned(
            style.families.to_owned(),
        ))));
        for property in style.common_properties() {
            styles.insert(property);
        }
        styles.insert(StyleProperty::TextWrapMode(if self.editor.multiline {
            parley::TextWrapMode::Wrap
        } else {
            parley::TextWrapMode::NoWrap
        }));
        self.engine().refresh_layout();
        self.rebuilt(false);
    }

    /// Reflow within a finite nonnegative width; `None` disables soft wrapping.
    /// Single-line editors keep soft wrapping disabled. Identical constraints do no work.
    /// Parley's editor currently reshapes on changed width/alignment.
    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) -> Result<Size, TextError> {
        if width.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err(TextError::InvalidWidth);
        }
        if self.editor.width != width || self.editor.alignment != alignment {
            self.editor.width = width;
            self.editor.alignment = alignment;
            self.editor.inner.set_width(width);
            self.editor.inner.set_alignment(alignment);
            self.engine().refresh_layout();
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
            self.editor.inner.set_text(text);
            self.engine().refresh_layout();
            self.engine().move_to_text_end();
            self.rebuilt(true);
        }
        self.editor.history.clear();
        Ok(())
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
            let caret = range.start;
            let mut start = 0;
            // Reuse Parley's ICU tables: shaping clusters can split graphemes.
            for end in icu_segmenter::GraphemeClusterSegmenter::new()
                .segment_str(self.editor.display_text())
                .skip(1)
            {
                if end > caret || (previous && end == caret) {
                    if !previous || caret != 0 {
                        range = start..end;
                    }
                    break;
                }
                start = end;
            }
        }
        self.editor.history.break_group();
        if !range.is_empty() {
            self.replace_range(range, "");
        }
        Ok(())
    }

    /// Select UTF-8 byte endpoints. Endpoints inside shaping clusters snap to the
    /// cluster start; this method is for user selection, not raw protocol edits.
    pub fn select(&mut self, selection: Selection) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        validate_selection(self.editor.display_text(), selection)?;
        let before = self.editor.inner.generation();
        self.engine()
            .select_byte_range(selection.anchor, selection.focus);
        self.moved(before);
        Ok(())
    }

    /// Move in visual order, optionally extending from the existing anchor.
    pub fn move_cursor(&mut self, movement: Movement, extend: bool) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        let before = self.editor.inner.generation();
        let mut d = self.engine();
        match (movement, extend) {
            (Movement::Left, false) => d.move_left(),
            (Movement::Left, true) => d.select_left(),
            (Movement::Right, false) => d.move_right(),
            (Movement::Right, true) => d.select_right(),
            (Movement::WordLeft, false) => d.move_word_left(),
            (Movement::WordLeft, true) => d.select_word_left(),
            (Movement::WordRight, false) => d.move_word_right(),
            (Movement::WordRight, true) => d.select_word_right(),
            (Movement::Up, false) => d.move_up(),
            (Movement::Up, true) => d.select_up(),
            (Movement::Down, false) => d.move_down(),
            (Movement::Down, true) => d.select_down(),
            (Movement::LineStart, false) => d.move_to_line_start(),
            (Movement::LineStart, true) => d.select_to_line_start(),
            (Movement::LineEnd, false) => d.move_to_line_end(),
            (Movement::LineEnd, true) => d.select_to_line_end(),
            (Movement::TextStart, false) => d.move_to_text_start(),
            (Movement::TextStart, true) => d.select_to_text_start(),
            (Movement::TextEnd, false) => d.move_to_text_end(),
            (Movement::TextEnd, true) => d.select_to_text_end(),
        }
        self.moved(before);
        Ok(())
    }

    /// Hit-test a point in the same local coordinates used to paint the layout.
    pub fn select_at(&mut self, point: Point, how: HitSelection) -> Result<(), TextError> {
        self.no_composition()?;
        self.ready()?;
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(TextError::InvalidPosition);
        }
        let before = self.editor.inner.generation();
        let mut d = self.engine();
        match how {
            HitSelection::Caret => d.move_to_point(point.x, point.y),
            HitSelection::Extend => d.extend_selection_to_point(point.x, point.y),
            HitSelection::Word => d.select_word_at_point(point.x, point.y),
            HitSelection::Line => d.select_line_at_point(point.x, point.y),
        }
        self.moved(before);
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
        let entry = if redo {
            self.editor.history.redo()
        } else {
            self.editor.history.undo()
        };
        let Some(entry) = entry else {
            return Ok(false);
        };
        let (range, text, selection) = if redo {
            (
                entry.start..entry.start + entry.removed.len(),
                entry.inserted.as_str(),
                entry.after,
            )
        } else {
            (
                entry.start..entry.start + entry.inserted.len(),
                entry.removed.as_str(),
                entry.before,
            )
        };
        let mut driver = self
            .editor
            .inner
            .driver(&mut self.system.fonts, &mut self.system.context);
        raw_replace(&mut driver, range, text);
        driver.select_byte_range(selection.anchor, selection.focus);
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
    pub(crate) fn ready(&self) -> Result<(), TextError> {
        if self.editor.diagnostics.unshaped_bytes != 0 {
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
        if self.editor.display_text()[range.clone()] == *text {
            if !range.is_empty() {
                let generation = self.editor.inner.generation();
                self.engine().move_to_byte(range.end);
                self.moved(generation);
            }
            return;
        }
        let removed = self.capture(&range, text);
        self.engine().insert_or_replace_selection(text);
        self.record(range.start, removed, text, before, merge);
        self.rebuilt(true);
    }
    pub(crate) fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let before = self.editor.selection();
        if self.editor.display_text()[range.clone()] == *text {
            return;
        }
        let removed = self.capture(&range, text);
        raw_replace(&mut self.engine(), range.clone(), text);
        self.record(range.start, removed, text, before, false);
        self.rebuilt(true);
    }
    fn capture(&mut self, range: &Range<usize>, text: &str) -> Option<String> {
        self.editor
            .history
            .prepare(range.len(), text.len())
            .then(|| self.editor.display_text()[range.clone()].to_owned())
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
        self.editor.diagnostics = diagnose(self.editor.display_text(), self.editor.layout());
        self.editor.changes.value |= value;
        self.editor.changes.layout = true;
        self.editor.changes.selection = true;
    }
    pub(crate) fn moved(&mut self, before: parley::Generation) {
        self.editor.history.break_group();
        self.editor.changes.selection |= before != self.editor.inner.generation();
    }
}

// Parley's public selection API snaps to shaping clusters. Its composition range
// accepts exact UTF-8 boundaries, also needed when undo removes a combining mark.
pub(crate) fn raw_replace(
    driver: &mut PlainEditorDriver<'_, Color>,
    range: Range<usize>,
    text: &str,
) {
    // The common path uses one rebuild. Exact scalar ranges inside a shaped
    // cluster use Parley's composition primitive instead of rounding the edit.
    if !driver.editor.is_composing() {
        let layout = driver.layout();
        let exact = [range.start, range.end].into_iter().all(|index| {
            parley::editing::Cursor::from_byte_index(layout, index, parley::Affinity::Downstream)
                .index()
                == index
        });
        if exact {
            driver.select_byte_range(range.start, range.end);
            driver.insert_or_replace_selection(text);
            return;
        }
    }
    driver.set_compose_byte_range(range.start, range.end);
    if text.is_empty() {
        driver.clear_compose();
    } else {
        driver.set_compose(text, Some((text.len(), text.len())));
        driver.finish_compose();
    }
}
