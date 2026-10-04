use std::{fmt, ops::Range};

use aegle_types::{Color, Rect, Size};
use parley::{BoundingBox, PlainEditor};

use crate::{
    Alignment, Layout, TextDiagnostics, TextError, TextStyle, TextSystem, history::History,
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
/// Use [`TextSystem::edit`] for changes. One Parley buffer/layout drives display,
/// hit testing and IME geometry; composition retains only the replaced fragment.
/// This is an editing model, not an OS input-method or clipboard connection.
pub struct Editor {
    pub(crate) inner: PlainEditor<Color>,
    pub(crate) composition: Option<Composition>,
    pub(crate) history: History,
    pub(crate) changes: EditChanges,
    pub(crate) diagnostics: TextDiagnostics,
    pub(crate) multiline: bool,
    pub(crate) read_only: bool,
    pub(crate) width: Option<f32>,
    pub(crate) alignment: Alignment,
}

impl Editor {
    /// The committed value, excluding preedit and including its replaced selection.
    pub fn text(&self) -> TextValue<'_> {
        if let Some(compose) = &self.composition {
            let range = self
                .inner
                .raw_compose()
                .as_ref()
                .expect("composition range");
            TextValue([
                &self.inner.raw_text()[..range.start],
                &compose.replaced,
                &self.inner.raw_text()[range.end..],
            ])
        } else {
            TextValue([self.inner.raw_text(), "", ""])
        }
    }

    /// Display buffer, including transient preedit. Never use it as a committed value.
    pub fn display_text(&self) -> &str {
        self.inner.raw_text()
    }
    /// Selection in the committed value; remains stable during composition.
    pub fn selection(&self) -> Selection {
        self.composition
            .as_ref()
            .map_or_else(|| self.visual_selection(), |c| c.original)
    }
    /// Selection in the display buffer, including the IME's current cursor request.
    pub fn visual_selection(&self) -> Selection {
        let s = self.inner.raw_selection();
        Selection {
            anchor: s.anchor().index(),
            focus: s.focus().index(),
        }
    }
    /// Selected committed text, or an empty slice for a collapsed selection.
    pub fn selected_text(&self) -> &str {
        self.composition.as_ref().map_or_else(
            || &self.inner.raw_text()[self.selection().range()],
            |c| c.replaced.as_str(),
        )
    }
    /// Preedit range in display-buffer UTF-8 bytes.
    pub fn composition_range(&self) -> Option<Range<usize>> {
        self.inner.raw_compose().clone()
    }
    /// Whether keyboard/IME modifications are disabled; selection remains available.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    /// Whether hard line breaks and wrapping are enabled.
    pub fn is_multiline(&self) -> bool {
        self.multiline
    }
    /// Layout shared by painting, hit testing and accessibility consumers.
    pub fn layout(&self) -> &Layout<Color> {
        self.inner.try_layout().expect("editor layout is current")
    }
    /// Current layout extent, excluding trailing whitespace.
    pub fn size(&self) -> Size {
        Size::new(self.layout().width(), self.layout().height())
    }
    /// Font coverage of the displayed buffer, including preedit.
    pub fn diagnostics(&self) -> TextDiagnostics {
        self.diagnostics
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
        self.inner
            .selection_geometry_with(|rect, _| visit(to_rect(rect)));
    }
    /// Caret geometry in local logical pixels; hidden when requested by the IME.
    pub fn caret_rect(&self, width: f32) -> Result<Option<Rect>, TextError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(TextError::InvalidPosition);
        }
        Ok(self.inner.cursor_geometry(width).map(to_rect))
    }
    /// Unclipped candidate-window anchor in local logical pixels. Hosts apply
    /// their scrolling/presentation transform, as for drawing and hit testing.
    pub fn ime_rect(&self) -> Rect {
        let mut area: Option<BoundingBox> = None;
        if let Some(range) = self.composition_range() {
            let selection = parley::Selection::new(
                parley::Cursor::from_byte_index(
                    self.layout(),
                    range.start,
                    parley::Affinity::Downstream,
                ),
                parley::Cursor::from_byte_index(
                    self.layout(),
                    range.end,
                    parley::Affinity::Upstream,
                ),
            );
            selection.geometry_with(self.layout(), |rect, _| {
                if let Some(area) = &mut area {
                    area.x0 = area.x0.min(rect.x0);
                    area.y0 = area.y0.min(rect.y0);
                    area.x1 = area.x1.max(rect.x1);
                    area.y1 = area.y1.max(rect.y1);
                } else {
                    area = Some(rect);
                }
            });
        }
        // Parley's IME helper clamps only one edge to wrapping width; unwrapped
        // single-line text can then produce a negative rectangle. Use actual
        // text geometry and let the host own viewport/scroll transformation.
        to_rect(area.unwrap_or_else(|| {
            self.inner
                .raw_selection()
                .focus()
                .geometry(self.layout(), 1.0)
        }))
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
        let mut editor = Editor {
            inner: PlainEditor::new(style.size),
            composition: None,
            history: History::new(options.history_bytes),
            changes: EditChanges::default(),
            diagnostics: TextDiagnostics::default(),
            multiline: options.multiline,
            read_only: false,
            width: None,
            alignment: Alignment::Start,
        };
        editor.inner.set_quantize(false);
        editor.inner.set_text(text);
        let mut driver = self.edit(&mut editor);
        driver.apply_style(style);
        driver.engine().move_to_text_end();
        if editor.diagnostics.unshaped_bytes != 0 {
            return Err(TextError::MissingFont);
        }
        Ok(editor)
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
