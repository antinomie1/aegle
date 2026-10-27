//! Selections across an editor's paragraphs: moving within one
//! paragraph's layout, and stepping between neighboring paragraphs.
//!
//! Word-wise movement and selection take their boundaries from the operating
//! system's segmenter ([`crate::segment`]) and from nothing else: Aegle bundles
//! no dictionary and keeps no second, approximate notion of a word. When the
//! platform has no segmenter the word operations do nothing, and
//! [`crate::segment::available`] says so; every other movement still works.
//!
//! Boundaries are logical, so stepping follows byte order in every script,
//! including right-to-left text.

use aegle_types::Point;

use crate::{
    document::{Caret, Document},
    layout::Affinity,
};

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
        if word {
            return match self.word_step(sel.focus, forward) {
                Some(focus) => sel.to(focus, extend),
                None => sel,
            };
        }
        let (block, local, affinity) = self.local_caret(sel.focus);
        let layout = &self.blocks[block].layout;
        let at_boundary = match forward {
            true => local >= self.blocks[block].range.len(),
            false => local == 0,
        };
        let (next_local, next_affinity) = if at_boundary {
            (local, affinity)
        } else {
            layout.move_cursor(local, affinity, forward)
        };
        let stuck = at_boundary || (next_local == local);
        let focus = match (stuck, forward) {
            (true, true) if block + 1 < self.blocks.len() => {
                Caret::downstream(self.blocks[block + 1].range.start)
            }
            (true, false) if block > 0 => Caret {
                index: self.blocks[block - 1].range.end,
                affinity: Affinity::Upstream,
            },
            _ => Caret {
                index: self.blocks[block].range.start
                    + next_local.min(self.blocks[block].range.len()),
                affinity: next_affinity,
            },
        };
        sel.to(focus, extend)
    }

    /// The neighboring display line at the kept horizontal position; past
    /// the first or last line, that line's start or end.
    pub fn vertical(&self, sel: Selected, down: bool, extend: bool) -> Selected {
        let (index, local, affinity) = self.local_caret(sel.focus);
        let block = &self.blocks[index];
        let geometry = block.layout.caret_geometry(local, affinity, 0.0);
        let h_pos = sel.h_pos.unwrap_or(geometry.x0 + block.origin.x);
        let lines = block.lines();
        let line = (0..lines)
            .position(|l| {
                let m = block.layout.line_metrics(l).unwrap_or_default();
                geometry.y0 < m.y + m.height
            })
            .unwrap_or(lines.saturating_sub(1));
        let target = match down {
            true if line + 1 < lines => Some((index, line + 1)),
            true if index + 1 < self.blocks.len() => Some((index + 1, 0)),
            false if line > 0 => Some((index, line - 1)),
            false if index > 0 => {
                Some((index - 1, self.blocks[index - 1].lines().saturating_sub(1)))
            }
            _ => None,
        };
        let focus = target.map(|(target, line)| {
            let b = &self.blocks[target];
            let m = b.layout.line_metrics(line).unwrap_or_default();
            let y = m.y + m.height * 0.5;
            let (target_local, target_aff) = b.layout.hit_test(h_pos - b.origin.x, y);
            Caret {
                index: b.range.start + target_local.min(b.range.len()),
                affinity: target_aff,
            }
        });
        let focus = focus.unwrap_or_else(|| self.line_edge(sel, down, false).focus);
        Selected {
            h_pos: Some(h_pos),
            ..sel.to(focus, extend)
        }
    }

    /// The start or end of the focus's display line.
    pub fn line_edge(&self, sel: Selected, end: bool, extend: bool) -> Selected {
        let (block, local, _) = self.local_caret(sel.focus);
        let b = &self.blocks[block];
        let line_range = b.layout.line_range_for_index(local);
        let (idx, affinity) = match end {
            true => (line_range.end, Affinity::Upstream),
            false => (line_range.start, Affinity::Downstream),
        };
        sel.to(
            Caret {
                index: b.range.start + idx,
                affinity,
            },
            extend,
        )
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
        let (anchor, focus) = match unit {
            Unit::Word => match self.word_at(block, at) {
                Some((anchor, focus)) => (anchor, focus),
                None => {
                    let caret = self.hit(point);
                    (caret, caret)
                }
            },
            Unit::Line => {
                let line_range = layout.line_range_from_point(at.x, at.y);
                (
                    Caret::downstream(self.blocks[block].range.start + line_range.start),
                    Caret {
                        index: self.blocks[block].range.start + line_range.end,
                        affinity: Affinity::Upstream,
                    },
                )
            }
        };
        Selected {
            anchor,
            focus,
            h_pos: None,
            base: Some((anchor, focus, unit)),
        }
    }

    /// The system's word at `at` in `block`, or `None` when the platform has no
    /// segmenter.
    fn word_at(&self, block: usize, at: Point) -> Option<(Caret, Caret)> {
        let range = self.blocks[block].range.clone();
        let text = &self.text[range.clone()];
        let bounds = crate::segment::boundaries(text)?;
        let (local, _) = self.blocks[block].layout.hit_test(at.x, at.y);
        let word = bounds.containing(local);
        Some((
            self.snap(range.start + word.start),
            Caret {
                index: range.start + word.end,
                affinity: Affinity::Upstream,
            },
        ))
    }

    /// The caret one word before or after `caret`, from the system's
    /// boundaries.
    fn word_step(&self, caret: Caret, forward: bool) -> Option<Caret> {
        let (block, local, _) = self.local_caret(caret);
        let range = self.blocks[block].range.clone();
        let text = &self.text[range.clone()];
        let bounds = crate::segment::boundaries(text)?;
        let local = local.clamp(0, range.len());
        match forward {
            true => bounds.next(local).map(|at| self.snap(range.start + at)),
            false => bounds.previous(local).map(|at| self.snap(range.start + at)),
        }
        .or_else(|| match forward {
            true => self
                .blocks
                .get(block + 1)
                .map(|next| Caret::downstream(next.range.start)),
            false => (block > 0).then(|| Caret {
                index: self.blocks[block - 1].range.end,
                affinity: Affinity::Upstream,
            }),
        })
    }

    /// Extends to `point`, by whole words or lines when the selection started
    /// with one.
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
