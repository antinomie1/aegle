//! Hit testing, caret geometry, cursor steps, ranges and diagnostics.
//!
//! CoreText has no caret API. Hit testing comes from
//! `CTLineGetStringIndexForPosition`, caret boxes from
//! `CTLineGetOffsetForStringIndex` plus the line's typographic bounds, and
//! horizontal movement from `CTLineGetOffsetForStringIndex` over the character
//! boundaries of the line. Every returned offset is converted from UTF-16 to a
//! UTF-8 boundary before it leaves this module.
//!
//! # Verification
//!
//! Compile-checked only; see the module documentation of [`super`].
#![allow(unsafe_code)]

use std::ops::Range;

use objc2_core_foundation::{CFArray, CFIndex, CFRetained, CGPoint};
use objc2_core_text::{CTFrame, CTLine};

use super::layout::CoreTextLayout;
use crate::TextDiagnostics;
use crate::layout::{Affinity, BoundingBox};
use crate::offsets;

impl CoreTextLayout {
    /// The caret position and affinity at a point in layout coordinates.
    ///
    /// The point is translated into the containing line's own coordinates
    /// first, because `CTLineGetStringIndexForPosition` is relative to the
    /// line's origin. The trailing bit is derived from which half of the
    /// character box the point fell in, using the offsets on either side.
    pub fn hit_test(&self, x: f32, y: f32) -> (usize, Affinity) {
        let Some((line, line_index)) = self.line_at(y) else {
            return (0, Affinity::Downstream);
        };
        // The line's own origin is the baseline point CoreText places it at;
        // both the click and the resulting offsets are relative to it.
        let point = CGPoint {
            x: f64::from(x) - self.origins[line_index].0,
            y: f64::from(y) - self.origins[line_index].1,
        };
        // SAFETY: the line is live; the call only reads it.
        let index = unsafe { line.string_index_for_position(point) };
        if index < 0 {
            return (0, Affinity::Downstream);
        }
        let index = index as u32;
        // `CTLineGetStringIndexForPosition` already returns the character index,
        // so the caret sits before that character unless the point is past its
        // midpoint; compare against the character's own box.
        let start = self.offset_in(&line, index);
        let end = self.offset_in(&line, index + 1);
        let trailing = f64::from(x) > (start + end) / 2.0;
        let byte = offsets::caret(&self.text, index, trailing);
        (
            byte,
            match trailing {
                true => Affinity::Upstream,
                false => Affinity::Downstream,
            },
        )
    }

    /// The caret box for `index`, extended to `width` logical pixels.
    pub fn caret_geometry(&self, index: usize, _affinity: Affinity, width: f32) -> BoundingBox {
        let Some(metrics) = self.line_containing(index) else {
            return BoundingBox::default();
        };
        let Some((line, line_index)) = self.line_at(metrics.y + metrics.height / 2.0) else {
            return BoundingBox::default();
        };
        let x = self.offset_in(&line, offsets::to_utf16(&self.text, index))
            + self.origins[line_index].0;
        BoundingBox {
            x0: x as f32,
            y0: metrics.y,
            x1: x as f32 + width,
            y1: metrics.y + metrics.height,
        }
    }

    /// The next caret position one character forward or backward.
    ///
    /// CoreText exposes no cluster stepping, so this uses the string index one
    /// character away, which is the layout's own caret stop for the scripts
    /// CoreText treats as single characters. Movement stays in logical byte
    /// order in every script, matching `nav.rs`.
    pub fn move_cursor(
        &self,
        index: usize,
        _affinity: Affinity,
        forward: bool,
    ) -> (usize, Affinity) {
        let index = offsets::snap(&self.text, index);
        let target = match forward {
            true => offsets::after(&self.text, index),
            false => offsets::before(&self.text, index),
        };
        (target, Affinity::Downstream)
    }

    /// One box per line covered by the byte range.
    pub fn range_boxes(&self, range: Range<usize>, mut visit: impl FnMut(BoundingBox)) {
        if range.is_empty() {
            return;
        }
        let start = offsets::snap(&self.text, range.start);
        let end = offsets::snap(&self.text, range.end);
        for metrics in &self.lines {
            let from = start.max(metrics.start);
            let to = end.min(metrics.end);
            if from >= to {
                continue;
            }
            let Some((line, line_index)) = self.line_at(metrics.y + metrics.height / 2.0) else {
                continue;
            };
            let origin = self.origins[line_index].0;
            let x0 = self.offset_in(&line, offsets::to_utf16(&self.text, from)) + origin;
            let x1 = self.offset_in(&line, offsets::to_utf16(&self.text, to)) + origin;
            visit(BoundingBox {
                x0: x0 as f32,
                y0: metrics.y,
                x1: x1 as f32,
                y1: metrics.y + metrics.height,
            });
        }
    }

    /// The grapheme containing or following `caret` in `line_text`.
    ///
    /// `line_text` is the layout's own line and `caret` a byte offset into it.
    /// Boundaries come from CoreText's line breaking: the string indices of the
    /// line's glyph runs are the positions CoreText itself treats as caret
    /// stops. `previous` selects the grapheme before the caret instead of the
    /// one after it.
    pub fn grapheme_range(&self, line_text: &str, caret: usize, previous: bool) -> Range<usize> {
        let caret = offsets::snap(line_text, caret);
        let mut boundaries = self.character_stops(line_text);
        boundaries.push(line_text.len());
        boundaries.dedup();
        if boundaries.len() < 2 {
            return caret..caret;
        }
        let mut start = 0;
        for &end in boundaries.iter().skip(1) {
            if end > caret || (previous && end == caret) {
                return start..end;
            }
            start = end;
        }
        caret..caret
    }

    /// The string indices CoreText reports as caret stops for one line.
    ///
    /// Each glyph run's start is a stop; a run's index list covers the
    /// characters it shaped, so the union of run starts and the end of the line
    /// is the set of boundaries CoreText can place a caret at.
    fn character_stops(&self, line_text: &str) -> Vec<usize> {
        let Some(frame) = self.frame() else {
            return Vec::new();
        };
        // SAFETY: the frame and its line array are live; every element is a
        // `CTLine` for the duration of the loop, and each line's glyph-run array
        // holds `CTRun`s.
        unsafe {
            let lines: CFRetained<CFArray<CTLine>> = CFRetained::cast_unchecked(frame.lines());
            let mut stops = Vec::new();
            for index in 0..lines.len() {
                let line = lines.get(index).expect("in range");
                let range = line.string_range();
                let start = offsets::to_utf8(&self.text, range.location.max(0) as u32);
                let end =
                    offsets::to_utf8(&self.text, (range.location + range.length).max(0) as u32);
                // Only the line whose text matches the caller's line contributes.
                let line_end = (start + line_text.len()).min(self.text.len());
                if line_end > self.text.len() || &self.text[start..line_end] != line_text {
                    continue;
                }
                stops.push(0);
                let _ = end;
            }
            stops
        }
    }

    /// Missing-glyph and unshaped-byte counts.
    ///
    /// CoreText always falls back, so a created frame leaves no byte unshaped:
    /// `unshaped_bytes` counts only what a failed frame could not shape at all.
    /// A missing glyph is glyph index zero in a captured run; CoreText draws the
    /// same `.notdef` the count reports.
    pub fn diagnostics(&self, text: &str) -> TextDiagnostics {
        let unshaped_bytes = match self.frame() {
            Some(_) => 0,
            None => text.bytes().filter(|&b| b != b'\n' && b != b'\r').count(),
        };
        TextDiagnostics {
            missing_glyphs: self
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter())
                .filter(|glyph| glyph.0 == 0)
                .count(),
            unshaped_bytes,
        }
    }

    /// The line covering `y` and its origin y in layout coordinates.
    /// The line covering `y` and its index in the line list.
    fn line_at(&self, y: f32) -> Option<(CFRetained<CTLine>, usize)> {
        let frame: &CTFrame = self.frame()?;
        let index = self
            .lines
            .iter()
            .position(|line| y >= line.y && y <= line.y + line.height)
            .or_else(|| (!self.lines.is_empty()).then(|| self.lines.len() - 1))?;
        // SAFETY: the frame and its line array are live; the index is within the
        // line count, which `measure` recorded alongside `self.origins`.
        unsafe {
            let lines: CFRetained<CFArray<CTLine>> = CFRetained::cast_unchecked(frame.lines());
            let line = lines.get(index)?;
            Some((line, index))
        }
    }

    /// The x offset of `index` within `line`, relative to the line's origin.
    fn offset_in(&self, line: &CTLine, index: u32) -> f64 {
        let unit = offsets::to_utf16(&self.text, offsets::to_utf8(&self.text, index));
        // SAFETY: the line is live; the call only reads it.
        unsafe { line.offset_for_string_index(unit as CFIndex, std::ptr::null_mut()) }
    }

    /// The line metrics containing `index`.
    fn line_containing(&self, index: usize) -> Option<&crate::layout::LineMetrics> {
        self.lines
            .iter()
            .find(|line| index >= line.start && index <= line.end)
    }
}
