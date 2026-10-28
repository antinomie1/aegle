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
    /// Boundaries come from CoreFoundation's composed-character segmentation
    /// (`CFStringGetRangeOfComposedCharactersAtIndex`), the same extended
    /// grapheme rule CoreText uses for editing. Only the caret's line is
    /// segmented: the string is built once for that line, so a paragraph of
    /// thousands of lines costs one line's worth of work per deletion.
    pub fn grapheme_range(&self, line_text: &str, caret: usize, previous: bool) -> Range<usize> {
        let caret = offsets::snap(line_text, caret);
        if line_text.is_empty() {
            return caret..caret;
        }
        let text = super::sys::cf_string(line_text);
        let units = line_text.encode_utf16().count();
        // The caret is already a UTF-8 boundary, so its UTF-16 index is exact.
        let unit = offsets::to_utf16(line_text, caret);
        let at = if previous && unit > 0 {
            // The character before the caret: step back one unit, then take the
            // composed range that contains that unit.
            unit - 1
        } else {
            unit
        };
        if at as usize >= units {
            return caret..caret;
        }
        // SAFETY: the string is live and `at` is within its length; the call
        // only reads the string.
        let range = unsafe { text.range_of_composed_characters_at_index(at as isize) };
        if range.location < 0 || range.length <= 0 {
            return caret..caret;
        }
        let start = offsets::to_utf8(line_text, range.location as u32);
        let end = offsets::to_utf8(line_text, (range.location + range.length) as u32);
        start..end
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
