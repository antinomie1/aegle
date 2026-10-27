//! An editor's text as one layout per hard line: an edit shapes only
//! the paragraphs it touches, rewrapping only breaks lines, and a new color
//! shapes nothing.

use std::ops::Range;

use aegle_types::{Color, Point, Size};

use crate::{
    Alignment, TextDiagnostics, TextSystem,
    layout::{Affinity, BoundingBox, Layout},
    style::Shaping,
};

/// A caret in the whole buffer: a byte index and the side it binds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Caret {
    pub index: usize,
    pub affinity: Affinity,
}

impl Caret {
    pub fn downstream(index: usize) -> Self {
        Self {
            index,
            affinity: Affinity::Downstream,
        }
    }
}

/// The text between two hard line breaks.
pub(crate) struct Block {
    /// The text before the break.
    pub range: Range<usize>,
    /// Byte length of the break that follows; zero for the last block.
    pub break_len: usize,
    pub layout: Layout,
    /// Top-left corner of the layout in the editor.
    pub origin: Point,
    /// Height of the lines before the empty one after the break.
    pub height: f32,
    pub diagnostics: TextDiagnostics,
    #[cfg(all(
        feature = "text-a11y",
        any(
            feature = "text-parley",
            not(any(target_os = "linux", target_os = "windows"))
        )
    ))]
    pub access: parley::LayoutAccessibility,
}

pub(crate) struct Document {
    /// Display buffer, including preedit.
    pub text: String,
    /// At least one, in text order.
    pub blocks: Vec<Block>,
    pub shaping: Shaping,
    pub color: Color,
    pub width: Option<f32>,
    pub alignment: Alignment,
    /// Underlined preedit range.
    pub compose: Option<Range<usize>>,
    /// Soft wrapping, for multiline editors.
    pub wrap: bool,
    pub size: Size,
    pub diagnostics: TextDiagnostics,
}

impl Document {
    pub fn new(
        system: &mut TextSystem,
        text: &str,
        shaping: Shaping,
        color: Color,
        wrap: bool,
    ) -> Self {
        let mut document = Self {
            text: text.to_owned(),
            blocks: Vec::new(),
            shaping,
            color,
            width: None,
            alignment: Alignment::Start,
            compose: None,
            wrap,
            size: Size::default(),
            diagnostics: TextDiagnostics::default(),
        };
        document.blocks = document.scan(system, 0, usize::MAX);
        document.place();
        document
    }

    /// Replaces `range` with `text` and underlines `compose`, which lies in
    /// the inserted text; shapes only the paragraphs holding the edit or the
    /// old preedit.
    pub fn splice(
        &mut self,
        system: &mut TextSystem,
        range: Range<usize>,
        text: &str,
        compose: Option<Range<usize>>,
    ) {
        let (mut low, mut high) = (range.start, range.end);
        if let Some(old) = &self.compose {
            (low, high) = (low.min(old.start), high.max(old.end));
        }
        let first = self.block_at(low);
        let mut last = self.block_at(high);
        if high > self.blocks[last].range.end && last + 1 < self.blocks.len() {
            last += 1;
        }
        let (removed, inserted) = (range.len(), text.len());
        let shift = |index: usize| index - removed + inserted;
        let until = self
            .blocks
            .get(last + 1)
            .map_or(usize::MAX, |b| shift(b.range.start));
        self.text.replace_range(range, text);
        self.compose = compose;
        let mut from = self.blocks[first].range.start;
        let bytes = self.text.as_bytes();
        if from > 0 && bytes[from - 1] == b'\r' && bytes.get(from) == Some(&b'\n') {
            from += 1;
        }
        let shaped = self.scan(system, from, until);
        let kept = first + shaped.len();
        self.blocks.splice(first..=last, shaped);
        for block in &mut self.blocks[kept..] {
            block.range = shift(block.range.start)..shift(block.range.end);
        }
        self.place();
    }

    /// Shapes every paragraph again, as after a font change.
    pub fn reshape(&mut self, system: &mut TextSystem) {
        self.blocks = self.scan(system, 0, usize::MAX);
        self.place();
    }

    /// Breaks and aligns lines again without shaping.
    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) {
        self.width = width;
        self.alignment = alignment;
        for block in &mut self.blocks {
            block.layout.reflow(width, alignment);
        }
        self.place();
    }

    /// Shapes the paragraphs from byte `from` until one would start at `until`.
    fn scan(&self, system: &mut TextSystem, mut from: usize, until: usize) -> Vec<Block> {
        let mut blocks = Vec::new();
        loop {
            let (end, next) = next_break(&self.text, from);
            let underline = self.compose.as_ref().and_then(|c| {
                let range = c.start.max(from)..c.end.min(end);
                (range.start < range.end).then(|| range.start - from..range.end - from)
            });
            let text = &self.text[from..next.unwrap_or(end)];
            let mut layout = system.shape_layout(text, &self.shaping, self.wrap, underline);
            layout.reflow(self.width, self.alignment);
            blocks.push(Block {
                range: from..end,
                break_len: next.map_or(0, |next| next - end),
                diagnostics: layout.diagnostics(text),
                layout,
                origin: Point::default(),
                height: 0.0,
                #[cfg(all(
                    feature = "text-a11y",
                    any(
                        feature = "text-parley",
                        not(any(target_os = "linux", target_os = "windows"))
                    )
                ))]
                access: Default::default(),
            });
            match next {
                Some(next) if next != until => from = next,
                _ => return blocks,
            }
        }
    }

    /// Stacks the paragraphs.
    fn place(&mut self) {
        let widest = self
            .blocks
            .iter()
            .map(|b| b.layout.width())
            .fold(0.0, f32::max);
        let mut top = 0.0;
        let mut diagnostics = TextDiagnostics::default();
        for block in &mut self.blocks {
            let rtl = block.layout.is_rtl();
            let share = match self.alignment {
                _ if self.width.is_some() => 0.0,
                Alignment::Left => 0.0,
                Alignment::Right => 1.0,
                Alignment::Center => 0.5,
                Alignment::End => f32::from(!rtl),
                Alignment::Start | Alignment::Justify => f32::from(rtl),
            };
            block.origin = Point::new((widest - block.layout.width()) * share, top);
            let lines = block.lines();
            block.height = match lines < block.layout.len() {
                true => block
                    .layout
                    .line_metrics(lines.saturating_sub(1))
                    .map_or(0.0, |m| m.y + m.height),
                false => block.layout.height(),
            };
            top += block.height;
            diagnostics.missing_glyphs += block.diagnostics.missing_glyphs;
            diagnostics.unshaped_bytes += block.diagnostics.unshaped_bytes;
        }
        self.size = Size::new(widest, top);
        self.diagnostics = diagnostics;
    }

    /// The paragraph holding byte `index`, or whose break does.
    pub fn block_at(&self, index: usize) -> usize {
        self.blocks.partition_point(|b| b.range.start <= index) - 1
    }

    pub fn local_caret(&self, caret: Caret) -> (usize, usize, Affinity) {
        let index = self.block_at(caret.index);
        let block = &self.blocks[index];
        let (local, affinity) = match caret.index > block.range.end {
            true => (block.range.len(), Affinity::Upstream),
            false => (caret.index - block.range.start, caret.affinity),
        };
        (index, local, affinity)
    }

    /// The caret at byte `index`, snapped to a cluster boundary.
    pub fn snap(&self, index: usize) -> Caret {
        let block = self.block_at(index);
        let range = &self.blocks[block].range;
        let affinity = match index >= range.end {
            true => Affinity::Upstream,
            false => Affinity::Downstream,
        };
        let local = index.min(range.end) - range.start;
        Caret {
            index: range.start + local,
            affinity,
        }
    }

    /// The paragraph at height `y` and `point` in its coordinates.
    pub fn local(&self, point: Point) -> (usize, Point) {
        let index = self
            .blocks
            .partition_point(|b| b.origin.y + b.height <= point.y)
            .min(self.blocks.len() - 1);
        let origin = self.blocks[index].origin;
        (index, Point::new(point.x - origin.x, point.y - origin.y))
    }

    /// The caret nearest `point`.
    pub fn hit(&self, point: Point) -> Caret {
        let (block, at) = self.local(point);
        let (local, affinity) = self.blocks[block].layout.hit_test(at.x, at.y);
        Caret {
            index: self.blocks[block].range.start + local.min(self.blocks[block].range.len()),
            affinity,
        }
    }

    /// The caret's rectangle `width` wide, in editor coordinates.
    pub fn caret_box(&self, caret: Caret, width: f32) -> BoundingBox {
        let (block, local, affinity) = self.local_caret(caret);
        let b = &self.blocks[block];
        offset(b.layout.caret_geometry(local, affinity, width), b.origin)
    }

    /// Visits the rectangles covering `range`, including a selected break.
    pub fn range_boxes(&self, range: Range<usize>, mut visit: impl FnMut(BoundingBox)) {
        if range.is_empty() {
            return;
        }
        let first = self.block_at(range.start);
        for block in &self.blocks[first..] {
            if block.range.start > range.end {
                break;
            }
            let shaped = block.shaped().end;
            let start = range.start.clamp(block.range.start, shaped) - block.range.start;
            let end = range.end.min(shaped) - block.range.start;
            block
                .layout
                .range_boxes(start..end, |rect| visit(offset(rect, block.origin)));
        }
    }
}

impl Block {
    /// Its text with the break after it, as shaped.
    pub fn shaped(&self) -> Range<usize> {
        self.range.start..self.range.end + self.break_len
    }

    /// Its display lines, without the empty one after its break.
    pub fn lines(&self) -> usize {
        self.layout
            .len()
            .saturating_sub(usize::from(self.break_len > 0 && self.layout.len() > 1))
    }
}

/// The end of the paragraph starting at `from` and the start of the next.
fn next_break(text: &str, from: usize) -> (usize, Option<usize>) {
    for (index, c) in text[from..].char_indices() {
        let index = from + index;
        let length = match c {
            '\r' if text.as_bytes().get(index + 1) == Some(&b'\n') => 2,
            '\r' | '\n' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => c.len_utf8(),
            _ => continue,
        };
        return (index, Some(index + length));
    }
    (text.len(), None)
}

fn offset(rect: BoundingBox, origin: Point) -> BoundingBox {
    BoundingBox {
        x0: rect.x0 + origin.x,
        y0: rect.y0 + origin.y,
        x1: rect.x1 + origin.x,
        y1: rect.y1 + origin.y,
    }
}
