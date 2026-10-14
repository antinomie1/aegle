use std::{fmt, ops::Range};

use aegle_types::{Color, Point, Rect, Size};
use parley::{Affinity, BoundingBox};

use crate::{
    Layout, TextDiagnostics, TextError, TextStyle, TextSystem,
    document::{Caret, Document},
    history::History,
    nav::Selected,
};

/// Directed UTF-8 byte selection. Reversed endpoints preserve selection direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// Fixed end of the selection.
    pub anchor: usize,
    /// Moving end of the selection; the caret when collapsed.
    pub focus: usize,
}

impl Selection {
    /// Returns the selected bytes in logical order.
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }
}

/// Changes accumulated until [`Editor::take_changes`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditChanges {
    /// The committed value changed; preedit updates alone never set this flag.
    pub value: bool,
    /// Text geometry changed and its host must measure/record again.
    pub layout: bool,
    /// Selection, caret visibility or composition decoration changed.
    pub selection: bool,
    /// Input policy changed, requiring semantic and platform IME state updates.
    pub policy: bool,
    /// Only the drawing changed, as for a new color: record again, without
    /// measuring.
    pub paint: bool,
}

/// Retained undo/redo storage, excluding spare deque slots and allocator overhead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryStats {
    /// Occupied entry headers plus allocated inserted/removed string capacities.
    pub bytes: usize,
    /// Available undo groups.
    pub undo_steps: usize,
    /// Available redo groups.
    pub redo_steps: usize,
}

/// Construction policy for a plain text editor.
#[derive(Clone, Copy, Debug)]
pub struct EditorOptions {
    /// Permit hard line separators and wrapping. Defaults to a single line.
    pub multiline: bool,
    /// Retained history budget, default 1 MiB; zero disables undo storage.
    pub history_bytes: usize,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            multiline: false,
            history_bytes: 1024 * 1024,
        }
    }
}

/// A borrowed committed value, kept stable while the IME replaces a selection.
///
/// Normally one slice; while composing it contains the prefix, original selected
/// text and suffix. Reading allocates nothing; use `to_string()` only when an
/// owned contiguous value is needed. The preedit itself is never included.
#[derive(Clone, Copy, Debug)]
pub struct TextValue<'a>(pub(crate) [&'a str; 3]);

impl<'a> TextValue<'a> {
    /// Borrow the ordered, possibly empty pieces of the committed value.
    pub fn parts(self) -> [&'a str; 3] {
        self.0
    }
    /// UTF-8 byte length.
    pub fn len(self) -> usize {
        self.0.iter().map(|part| part.len()).sum()
    }
    /// Whether the value is empty.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for TextValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in self.0 {
            f.write_str(part)?;
        }
        Ok(())
    }
}

impl PartialEq<&str> for TextValue<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.0.iter().flat_map(|s| s.bytes()).eq(other.bytes())
    }
}

pub(crate) struct Composition {
    pub original: Selection,
    pub replaced: String,
}

/// Retained plain text, selection, preedit and bounded delta history.
///
/// Use [`TextSystem::edit`] for changes. The text is kept as one Parley layout
/// per hard line: an edit shapes only the lines it touches, a new wrap width
/// only breaks lines again, and a new color shapes nothing. The same layouts
/// drive display, hit testing and IME geometry; composition retains only the
/// replaced fragment. This is an editing model, not an OS input-method or
/// clipboard connection.
pub struct Editor {
    pub(crate) doc: Document,
    pub(crate) sel: Selected,
    /// The IME can hide the caret during composition.
    pub(crate) show_cursor: bool,
    /// Emitted run IDs, their character counts and paragraphs.
    #[cfg(feature = "text-a11y")]
    pub(crate) access_runs: Vec<(accesskit::NodeId, usize, usize)>,
    pub(crate) composition: Option<Composition>,
    pub(crate) history: History,
    pub(crate) changes: EditChanges,
    pub(crate) multiline: bool,
    pub(crate) read_only: bool,
    /// Committed password value; the display buffer then holds only masks.
    pub(crate) secret: Option<String>,
}

impl Editor {
    /// The committed value, excluding preedit and including its replaced selection.
    pub fn text(&self) -> TextValue<'_> {
        let text = self.doc.text.as_str();
        if let Some(secret) = &self.secret {
            TextValue([secret, "", ""])
        } else if let Some(compose) = &self.composition {
            let range = self.doc.compose.as_ref().expect("composition range");
            TextValue([&text[..range.start], &compose.replaced, &text[range.end..]])
        } else {
            TextValue([text, "", ""])
        }
    }

    /// Display buffer, including transient preedit. Never use it as a committed value.
    /// Password editors display one mask per character; selections and other
    /// byte offsets always refer to this buffer.
    pub fn display_text(&self) -> &str {
        &self.doc.text
    }
    /// Selection in the committed value; remains stable during composition.
    pub fn selection(&self) -> Selection {
        self.composition
            .as_ref()
            .map_or_else(|| self.visual_selection(), |c| c.original)
    }
    /// Selection in the display buffer, including the IME's current cursor request.
    pub fn visual_selection(&self) -> Selection {
        Selection {
            anchor: self.sel.anchor.index,
            focus: self.sel.focus.index,
        }
    }
    /// Selected committed text, or an empty slice for a collapsed selection.
    /// Password editors return the masked display slice, never the value.
    pub fn selected_text(&self) -> &str {
        self.composition.as_ref().map_or_else(
            || &self.doc.text[self.selection().range()],
            |c| c.replaced.as_str(),
        )
    }
    /// Preedit range in display-buffer UTF-8 bytes.
    pub fn composition_range(&self) -> Option<Range<usize>> {
        self.doc.compose.clone()
    }
    /// Whether keyboard/IME modifications are disabled; selection remains available.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    /// Whether the display masks the value; see [`crate::EditorDriver::set_password`].
    pub fn is_password(&self) -> bool {
        self.secret.is_some()
    }
    /// Whether hard line breaks and wrapping are enabled.
    pub fn is_multiline(&self) -> bool {
        self.multiline
    }
    /// The layout of each hard line with its top-left corner, in order; the
    /// same layouts painting, hit testing and accessibility use.
    pub fn layouts(&self) -> impl Iterator<Item = (Point, &Layout<Color>)> {
        self.doc
            .blocks
            .iter()
            .map(|block| (block.origin, &block.layout))
    }
    /// Current layout extent, excluding trailing whitespace.
    pub fn size(&self) -> Size {
        self.doc.size
    }
    /// Offset of the first line's baseline from the top, if there is a line.
    pub fn first_baseline(&self) -> Option<f32> {
        Some(self.doc.blocks[0].layout.lines().next()?.metrics().baseline)
    }
    /// Font coverage of the displayed buffer, including preedit.
    pub fn diagnostics(&self) -> TextDiagnostics {
        self.doc.diagnostics
    }
    /// The changes accumulated since the last [`Self::take_changes`],
    /// without draining them: a control compares them around one input to
    /// report an edit.
    pub fn changes(&self) -> EditChanges {
        self.changes
    }
    /// Drain accumulated invalidation flags without allocating.
    pub fn take_changes(&mut self) -> EditChanges {
        std::mem::take(&mut self.changes)
    }
    /// Retained delta history accounting.
    pub fn history_stats(&self) -> HistoryStats {
        self.history.stats()
    }
    /// Change the history budget, dropping oldest complete operations as necessary.
    pub fn set_history_limit(&mut self, bytes: usize) {
        self.history.set_limit(bytes);
    }
    /// End typing coalescing, for example at a paste, focus change or input pause.
    pub fn break_undo_group(&mut self) {
        self.history.break_group();
    }
    /// Visit selection rectangles without allocating an intermediate vector.
    pub fn selection_rects(&self, mut visit: impl FnMut(Rect)) {
        let range = self.visual_selection().range();
        self.doc.range_boxes(range, |rect| visit(to_rect(rect)));
    }
    /// Caret geometry in local logical pixels; hidden when requested by the IME.
    pub fn caret_rect(&self, width: f32) -> Result<Option<Rect>, TextError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(TextError::InvalidPosition);
        }
        let caret = self
            .show_cursor
            .then(|| self.doc.caret_box(self.sel.focus, width));
        Ok(caret.map(to_rect))
    }
    /// Unclipped candidate-window anchor in local logical pixels. Hosts apply
    /// their scrolling/presentation transform, as for drawing and hit testing.
    pub fn ime_rect(&self) -> Rect {
        let mut area: Option<BoundingBox> = None;
        if let Some(range) = self.composition_range() {
            self.doc.range_boxes(range, |rect| {
                let area = area.get_or_insert(rect);
                *area = area.union(rect);
            });
        }
        to_rect(area.unwrap_or_else(|| self.doc.caret_box(self.sel.focus, 1.0)))
    }
}

impl TextSystem {
    /// Creates an editor using this system's shared fonts and shaping scratch.
    pub fn editor(
        &mut self,
        text: &str,
        style: &TextStyle<'_>,
        options: EditorOptions,
    ) -> Result<Editor, TextError> {
        style.validate()?;
        validate_content(text, options.multiline)?;
        let shaping = self.shaping(style);
        let doc = Document::new(self, text, shaping, style.color, options.multiline);
        if doc.diagnostics.unshaped_bytes != 0 {
            return Err(TextError::MissingFont);
        }
        let end = Caret {
            index: text.len(),
            affinity: Affinity::Upstream,
        };
        Ok(Editor {
            doc,
            sel: Selected::collapsed(end),
            show_cursor: true,
            #[cfg(feature = "text-a11y")]
            access_runs: Vec::new(),
            composition: None,
            history: History::new(options.history_bytes),
            changes: EditChanges::default(),
            multiline: options.multiline,
            read_only: false,
            secret: None,
        })
    }

    /// Borrows an editor and this system for a short imperative update sequence.
    pub fn edit<'a>(&'a mut self, editor: &'a mut Editor) -> crate::EditorDriver<'a> {
        crate::EditorDriver {
            editor,
            system: self,
        }
    }
}

pub(crate) fn validate_content(text: &str, multiline: bool) -> Result<(), TextError> {
    if text.len() > u32::MAX as usize {
        return Err(TextError::TextTooLong);
    }
    if !multiline
        && text.contains([
            '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
    {
        return Err(TextError::SingleLine);
    }
    Ok(())
}

pub(crate) fn validate_selection(text: &str, selection: Selection) -> Result<(), TextError> {
    if !text.is_char_boundary(selection.anchor) || !text.is_char_boundary(selection.focus) {
        return Err(TextError::InvalidRange);
    }
    Ok(())
}

pub(crate) fn to_rect(b: BoundingBox) -> Rect {
    Rect::new(
        b.x0 as f32,
        b.y0 as f32,
        (b.x1 - b.x0) as f32,
        (b.y1 - b.y0) as f32,
    )
}
