//! Selections across an editor's paragraphs: Parley moves within one
//! paragraph's layout, and these step between neighboring paragraphs.

use aegle_types::Point;
use parley::{Affinity, Cursor};

use crate::document::{Caret, Document};

/// What a pointer selection selects and dragging extends by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Word,
    Line,
}

/// A directed selection in the display buffer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Selected {
    pub anchor: Caret,
    pub focus: Caret,
    /// Horizontal position vertical movement keeps, in editor coordinates.
    pub h_pos: Option<f32>,
    /// The word or line a pointer selection started with.
    pub base: Option<(Caret, Caret, Unit)>,
}

impl Selected {
    pub fn collapsed(caret: Caret) -> Self {
        Self {
            anchor: caret,
            focus: caret,
            h_pos: None,
            base: None,
        }
    }

    fn to(self, focus: Caret, extend: bool) -> Self {
        match extend {
            true => Self {
                focus,
                h_pos: None,
                base: None,
                ..self
            },
            false => Self::collapsed(focus),
        }
    }

    /// Whether the endpoints equal `other`'s.
    pub fn same(&self, other: &Self) -> bool {
        (self.anchor, self.focus) == (other.anchor, other.focus)
    }
}

impl Document {
    /// One cluster or word in visual order. Collapsing a selection without
    /// extending moves to its visually first or last end.
    pub fn horizontal(&self, sel: Selected, forward: bool, word: bool, extend: bool) -> Selected {
        if !extend && !word && sel.anchor.index != sel.focus.index {
            let (a, f) = (
                self.caret_box(sel.anchor, 0.0),
                self.caret_box(sel.focus, 0.0),
            );
            let anchor_later = (a.y0, a.x0) > (f.y0, f.x0);
            let anchor = match forward {
                true => anchor_later,
                false => (a.y0, a.x0) < (f.y0, f.x0),
            };
            return Selected::collapsed(if anchor { sel.anchor } else { sel.focus });
        }
        let (block, cursor) = self.cursor(sel.focus);
        let layout = &self.blocks[block].layout;
        let next = match (forward, word) {
            (true, false) => cursor.next_visual(layout),
            (false, false) => cursor.previous_visual(layout),
            (true, true) => cursor.next_visual_word(layout),
            (false, true) => cursor.previous_visual_word(layout),
        };
        let stuck = (next.index(), next.affinity()) == (cursor.index(), cursor.affinity());
        let focus = match (stuck, forward) {
            (true, true) if block + 1 < self.blocks.len() => {
                Caret::downstream(self.blocks[block + 1].range.start)
            }
            (true, false) if block > 0 => Caret {
                index: self.blocks[block - 1].range.end,
                affinity: Affinity::Upstream,
            },
            _ => self.caret(block, next),
        };
        sel.to(focus, extend)
    }

    /// The neighboring display line at the kept horizontal position; past
    /// the first or last line, that line's start or end.
    pub fn vertical(&self, sel: Selected, down: bool, extend: bool) -> Selected {
        let (index, cursor) = self.cursor(sel.focus);
        let block = &self.blocks[index];
        let geometry = cursor.geometry(&block.layout, 0.0);
        let h_pos = sel.h_pos.unwrap_or(geometry.x0 as f32 + block.origin.x);
        let lines = block.lines();
        let line = block
            .layout
            .lines()
            .position(|l| (geometry.y0 as f32) < l.metrics().block_max_coord)
            .unwrap_or(lines.saturating_sub(1));
        let target = match down {
            true if line + 1 < lines => Some((index, line + 1)),
            true if index + 1 < self.blocks.len() => Some((index + 1, 0)),
            false if line > 0 => Some((index, line - 1)),
            false if index > 0 => Some((index - 1, self.blocks[index - 1].lines() - 1)),
            _ => None,
        };
        let focus = target.and_then(|(target, line)| {
            let block = &self.blocks[target];
            let metrics = *block.layout.get(line)?.metrics();
            let y = metrics.block_max_coord - metrics.ascent * 0.5;
            let cursor = Cursor::from_point(&block.layout, h_pos - block.origin.x, y);
            Some(self.caret(target, cursor))
        });
        let focus = focus.unwrap_or_else(|| self.line_edge(sel, down, false).focus);
        Selected {
            h_pos: Some(h_pos),
            ..sel.to(focus, extend)
        }
    }

    /// The start or end of the focus's display line.
    pub fn line_edge(&self, sel: Selected, end: bool, extend: bool) -> Selected {
        let (block, cursor) = self.cursor(sel.focus);
        let layout = &self.blocks[block].layout;
        let line = parley::Selection::from(cursor);
        let line = match end {
            true => line.line_end(layout, false),
            false => line.line_start(layout, false),
        };
        sel.to(self.caret(block, line.focus()), extend)
    }

    /// The start or end of the whole text.
    pub fn text_edge(&self, sel: Selected, end: bool, extend: bool) -> Selected {
        let focus = match end {
            true => Caret {
                index: self.text.len(),
                affinity: Affinity::Upstream,
            },
            false => Caret::downstream(0),
        };
        sel.to(focus, extend)
    }

    /// The word or display line at `point`.
    pub fn unit_at(&self, point: Point, unit: Unit) -> Selected {
        let (block, at) = self.local(point);
        let layout = &self.blocks[block].layout;
        let selection = match unit {
            Unit::Word => parley::Selection::word_from_point(layout, at.x, at.y),
            Unit::Line => parley::Selection::line_from_point(layout, at.x, at.y),
        };
        let anchor = self.caret(block, selection.anchor());
        let focus = self.caret(block, selection.focus());
        Selected {
            anchor,
            focus,
            h_pos: None,
            base: Some((anchor, focus, unit)),
        }
    }

    /// Extends to `point`, by whole words or lines when the selection started
    /// with one, as Parley does.
    pub fn extend_to(&self, sel: Selected, point: Point) -> Selected {
        let Some((start, end, unit)) = sel.base else {
            return sel.to(self.hit(point), true);
        };
        let target = self.unit_at(point, unit);
        let right = target.anchor.index >= start.index;
        let carets = [target.anchor, target.focus, start, end];
        let min = carets.into_iter().min_by_key(|c| c.index).unwrap();
        let max = carets.into_iter().rev().max_by_key(|c| c.index).unwrap();
        let (anchor, focus) = if right { (min, max) } else { (max, min) };
        Selected {
            anchor,
            focus,
            h_pos: None,
            base: sel.base,
        }
    }
}
