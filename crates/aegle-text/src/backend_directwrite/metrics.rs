//! Hit testing, caret geometry, cursor steps, ranges and diagnostics.
//!
//! DirectWrite has no caret API. Hit testing comes from `HitTestPoint`, caret
//! boxes from `HitTestTextPosition`, and horizontal movement from the cluster
//! metrics: a cluster is one or more characters a shaper treats as a unit, so
//! stepping across cluster boundaries is the platform's own idea of a caret
//! stop. Every returned offset is converted from UTF-16 to a UTF-8 boundary
//! before it leaves this module.
#![allow(unsafe_code)]

use std::ops::Range;

use windows::Win32::Globalization::{
    SCRIPT_ANALYSIS, SCRIPT_ITEM, SCRIPT_LOGATTR, ScriptBreak, ScriptItemize,
};
use windows::Win32::Graphics::DirectWrite::{DWRITE_CLUSTER_METRICS, DWRITE_HIT_TEST_METRICS};
use windows::core::BOOL;
use windows::core::PCWSTR;

use super::layout::DirectWriteLayout;
use crate::TextDiagnostics;
use crate::layout::{Affinity, BoundingBox};
use crate::offsets;

impl DirectWriteLayout {
    /// The caret position and affinity at a point in layout coordinates.
    pub fn hit_test(&self, x: f32, y: f32) -> (usize, Affinity) {
        let Some(layout) = self.raw() else {
            return (0, Affinity::Downstream);
        };
        let mut trailing = BOOL(0);
        let mut inside = BOOL(0);
        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        // SAFETY: the layout is live and the out pointers refer to locals.
        let hit = unsafe { layout.HitTestPoint(x, y, &mut trailing, &mut inside, &mut metrics) };
        if hit.is_err() {
            return (0, Affinity::Downstream);
        }
        let index = offsets::caret(&self.text, metrics.textPosition, trailing.as_bool());
        let affinity = match trailing.as_bool() {
            true => Affinity::Upstream,
            false => Affinity::Downstream,
        };
        (index, affinity)
    }

    /// The caret box for `index`, extended to `width` logical pixels.
    pub fn caret_geometry(&self, index: usize, _affinity: Affinity, width: f32) -> BoundingBox {
        let Some(layout) = self.raw() else {
            return BoundingBox::default();
        };
        let unit = offsets::to_utf16(&self.text, index);
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        // SAFETY: the layout is live and the out pointers refer to locals.
        let hit = unsafe { layout.HitTestTextPosition(unit, false, &mut x, &mut y, &mut metrics) };
        if hit.is_err() {
            return BoundingBox::default();
        }
        let height = match self.line_containing(index) {
            Some(line) => line.height,
            None => metrics.height,
        };
        BoundingBox {
            x0: x,
            y0: y,
            x1: x + width,
            y1: y + height,
        }
    }

    /// The next caret position one cluster forward or backward.
    ///
    /// Movement stays in logical byte order in every script, matching
    /// `nav.rs`: a right-to-left cluster's visual next stop is still its
    /// logical successor here, so arrow keys do not reverse inside a paragraph.
    pub fn move_cursor(
        &self,
        index: usize,
        _affinity: Affinity,
        forward: bool,
    ) -> (usize, Affinity) {
        let Some(layout) = self.raw() else {
            return (index, Affinity::Downstream);
        };
        let mut count = 0u32;
        // SAFETY: a null buffer asks for the cluster count only.
        if unsafe { layout.GetClusterMetrics(None, &mut count) }.is_err() || count == 0 {
            return (index, Affinity::Downstream);
        }
        let mut clusters = vec![DWRITE_CLUSTER_METRICS::default(); count as usize];
        // SAFETY: the buffer holds exactly `count` entries from the first call.
        if unsafe { layout.GetClusterMetrics(Some(&mut clusters), &mut count) }.is_err() {
            return (index, Affinity::Downstream);
        }
        let unit = offsets::to_utf16(&self.text, index) as usize;
        // Walk clusters in logical order and step to the neighbouring boundary.
        let mut start = 0usize;
        for cluster in &clusters[..count as usize] {
            let end = start + cluster.length as usize;
            if unit >= start && unit < end {
                let target = match forward {
                    true => end,
                    false => start,
                };
                return (
                    offsets::to_utf8(&self.text, target as u32),
                    Affinity::Downstream,
                );
            }
            start = end;
        }
        (index, Affinity::Downstream)
    }

    /// One box per line covered by the byte range.
    pub fn range_boxes(&self, range: Range<usize>, mut visit: impl FnMut(BoundingBox)) {
        let Some(layout) = self.raw() else {
            return;
        };
        if range.is_empty() {
            return;
        }
        let start = offsets::to_utf16(&self.text, range.start);
        let end = offsets::to_utf16(&self.text, range.end);
        let length = end.saturating_sub(start);
        if length == 0 {
            return;
        }
        let mut count = 0u32;
        // SAFETY: a null buffer asks for the required metric count.
        let first = unsafe { layout.HitTestTextRange(start, length, 0.0, 0.0, None, &mut count) };
        if first.is_err() || count == 0 {
            return;
        }
        let mut metrics = vec![DWRITE_HIT_TEST_METRICS::default(); count as usize];
        // SAFETY: the buffer holds exactly `count` entries, which is what the
        // first call reported for this same range.
        let second = unsafe {
            layout.HitTestTextRange(start, length, 0.0, 0.0, Some(&mut metrics), &mut count)
        };
        if second.is_err() {
            return;
        }
        // Merge the fragments that share a line so a selection paints one box
        // per line, not one per shaping run.
        let mut current: Option<BoundingBox> = None;
        for metric in &metrics[..count as usize] {
            let box_ = BoundingBox {
                x0: metric.left,
                y0: metric.top,
                x1: metric.left + metric.width,
                y1: metric.top + metric.height,
            };
            match &mut current {
                Some(open) if (open.y0 - box_.y0).abs() < 1.0 => {
                    open.x0 = open.x0.min(box_.x0);
                    open.x1 = open.x1.max(box_.x1);
                }
                Some(_) => {
                    visit(current.take().expect("open box"));
                    current = Some(box_);
                }
                None => current = Some(box_),
            }
        }
        if let Some(open) = current {
            visit(open);
        }
    }

    /// The grapheme containing or following `caret` in `line_text`.
    ///
    /// `line_text` is the layout's own line and `caret` a byte offset into it.
    /// Boundaries come from Uniscribe's `ScriptBreak`, the same character-stop
    /// attribute the Pango backend takes from `pango_get_log_attrs`. Uniscribe
    /// needs the script of the item the character belongs to, so the line is
    /// itemized first with `ScriptItemize` and the analysis of the item that
    /// contains the caret is passed in; a zeroed analysis would segment complex
    /// scripts as if they were Latin.
    ///
    /// Only the caret's line is segmented, and the scan stops at the first
    /// boundary past the caret.
    pub fn grapheme_range(&self, line_text: &str, caret: usize, previous: bool) -> Range<usize> {
        let caret = offsets::snap(line_text, caret);
        let units: Vec<u16> = line_text.encode_utf16().collect();
        if units.is_empty() {
            return caret..caret;
        }
        let mut attrs = vec![SCRIPT_LOGATTR::default(); units.len()];
        let Some(analysis) = script_analysis(&units, caret, line_text) else {
            return caret..caret;
        };
        // SAFETY: the pointer and count describe the live `units` slice, and
        // `attrs` has one entry per code unit; ScriptBreak only writes `attrs`.
        let broke = unsafe {
            ScriptBreak(
                PCWSTR(units.as_ptr()),
                units.len() as i32,
                &analysis,
                attrs.as_mut_ptr(),
            )
        };
        if broke.is_err() {
            return caret..caret;
        }
        let mut breaks = Vec::new();
        let mut byte = 0usize;
        for (units_seen, attr) in attrs.iter().enumerate() {
            // `fCharStop` is bit 0 of SCRIPT_LOGATTR. Attributes are per code
            // unit; a low surrogate is skipped, its pair's start carries the
            // character stop.
            let unit = units[units_seen];
            let low_surrogate = (0xdc00..=0xdfff).contains(&unit);
            if attr._bitfield & 1 != 0 && !low_surrogate {
                breaks.push(offsets::to_utf8(line_text, byte as u32));
            }
            byte += match unit {
                0xd800..=0xdbff => 2,
                _ => 1,
            };
        }
        breaks.push(line_text.len());
        breaks.dedup();

        let mut start = 0;
        for &end in breaks.iter().skip(1) {
            if end > caret || (previous && end == caret) {
                return start..end;
            }
            start = end;
        }
        caret..caret
    }

    /// Missing-glyph and unshaped-byte counts.
    ///
    /// DirectWrite always falls back, so a created layout leaves no byte
    /// unshaped: `unshaped_bytes` counts only what a *failed* layout could not
    /// shape at all, which is every byte but the line separators. That keeps a
    /// text system with no usable font collection reporting honestly instead of
    /// claiming success. A missing glyph is glyph index zero (`.notdef`) in a
    /// captured run; DirectWrite draws the same `.notdef` box the count
    /// reports, so the count and the pixels agree.
    pub fn diagnostics(&self, text: &str) -> TextDiagnostics {
        let unshaped_bytes = match self.raw() {
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

    /// The line whose byte range contains `index`, if any.
    fn line_containing(&self, index: usize) -> Option<&crate::layout::LineMetrics> {
        self.lines
            .iter()
            .find(|line| index >= line.start && index <= line.end)
    }
}

/// The Uniscribe analysis of the script item containing `caret`.
///
/// `ScriptItemize` splits the line into runs of one script and direction; the
/// analysis of the run covering the caret is what `ScriptBreak` needs. Returns
/// `None` when itemization fails or reports no item, so the caller leaves the
/// caret where it was instead of segmenting with a guessed script.
fn script_analysis(units: &[u16], caret: usize, line_text: &str) -> Option<SCRIPT_ANALYSIS> {
    // The caret in UTF-16 units, then the item containing it.
    let unit = offsets::to_utf16(line_text, caret) as usize;
    // Uniscribe's documented worst case is one item per code unit plus one.
    let mut items = vec![SCRIPT_ITEM::default(); units.len() + 1];
    let mut count = 0i32;
    // SAFETY: the slice and its capacity describe live storage; ScriptItemize
    // writes at most `items.len()` entries and reports how many it wrote.
    let itemized = unsafe { ScriptItemize(units, None, None, &mut items, &mut count) };
    if itemized.is_err() || count <= 0 {
        return None;
    }
    let count = count as usize;
    let mut found = items[0].a;
    for index in 0..count {
        let start = items[index].iCharPos.max(0) as usize;
        let end = match items.get(index + 1) {
            Some(next) => next.iCharPos.max(0) as usize,
            None => units.len(),
        };
        if unit >= start && unit <= end {
            found = items[index].a;
            break;
        }
    }
    Some(found)
}
