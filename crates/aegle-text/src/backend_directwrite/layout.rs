//! Retained DirectWrite layout: shaping, line breaking and measured geometry.
//!
//! A `DirectWriteLayout` owns one `IDWriteTextLayout`, created for the shaping
//! key and re-created when the key changes. Reflow only changes the maximum
//! width and alignment, which DirectWrite applies without Aegle re-shaping.
//! Line metrics, hit testing, caret geometry and painting all read the same
//! object, so they cannot disagree about where a character is.
#![allow(unsafe_code)]

use std::ops::Range;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_LINE_METRICS, DWRITE_TEXT_METRICS, IDWriteTextLayout,
};

use super::sys::{alignment as dwrite_alignment, factory, wrapping};
use super::system::DirectWriteSystem;
use crate::layout::LineMetrics;
use crate::offsets;
use crate::style::Shaping;
use crate::{Alignment, ContentWidths};

/// A retained DirectWrite layout and the geometry measured from it.
pub(crate) struct DirectWriteLayout {
    /// The COM layout; `None` for the empty layout and for a text system with
    /// no usable factory.
    raw: Option<IDWriteTextLayout>,
    /// UTF-8 text; DirectWrite indexes it in UTF-16 code units.
    pub(super) text: String,
    pub(super) width: Option<f32>,
    pub(super) wrap: bool,
    pub(super) size: (f32, f32),
    pub(super) lines: Vec<LineMetrics>,
    pub(super) first_baseline: Option<f32>,
    pub(super) content_widths: ContentWidths,
    pub(super) is_rtl: bool,
    /// Underline range the layout was built with, kept for rebuilds.
    underline: Option<Range<usize>>,
    /// Glyph runs captured for painting and diagnostics.
    pub(super) runs: Vec<super::runs::StoredRun>,
}

impl DirectWriteLayout {
    /// Shapes `text` with `shaping` and lays it out without soft wrapping.
    pub fn new(
        system: &mut DirectWriteSystem,
        text: &str,
        shaping: &Shaping,
        wrap: bool,
        underline: Option<Range<usize>>,
    ) -> Self {
        let mut layout = Self::empty();
        layout.text = text.to_owned();
        layout.wrap = wrap;
        layout.underline = underline.filter(|range| !range.is_empty());
        layout.build(system, shaping);
        layout
    }

    /// An empty layout, used for released paragraphs.
    pub fn empty() -> Self {
        Self {
            raw: None,
            text: String::new(),
            width: None,
            wrap: false,
            size: (0.0, 0.0),
            lines: Vec::new(),
            first_baseline: None,
            content_widths: ContentWidths::default(),
            is_rtl: false,
            underline: None,
            runs: Vec::new(),
        }
    }

    /// Creates the COM layout for `shaping` and measures it.
    fn build(&mut self, system: &DirectWriteSystem, shaping: &Shaping) {
        self.raw = None;
        let Some(format) = system.format(shaping) else {
            return;
        };
        let Some(factory) = factory() else {
            return;
        };
        let units: Vec<u16> = self.text.encode_utf16().collect();
        // SAFETY: the text slice and format outlive the call; the returned
        // layout is a reference-counted COM object.
        let Ok(layout) = (unsafe { factory.CreateTextLayout(&units, &format, f32::MAX, f32::MAX) })
        else {
            return;
        };
        // SAFETY: the layout is live; the setters below only change layout state.
        unsafe {
            let _ = layout.SetWordWrapping(wrapping(self.wrap));
            let _ = layout.SetTextAlignment(dwrite_alignment(Alignment::Start));
            if let Some(range) = self.underline.clone() {
                let start = offsets::to_utf16(&self.text, range.start);
                let end = offsets::to_utf16(&self.text, range.end);
                let _ = layout.SetUnderline(
                    true,
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE {
                        startPosition: start,
                        length: end.saturating_sub(start),
                    },
                );
            }
        }
        self.raw = Some(layout);
        self.reflow(self.width, Alignment::Start);
        self.capture_runs();
    }

    /// Applies wrapping width and alignment, then re-measures.
    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) {
        self.width = width;
        let Some(layout) = &self.raw else {
            return;
        };
        // SAFETY: the layout is live; both setters only change layout state.
        unsafe {
            let max_width = match (self.wrap, width) {
                (true, Some(width)) => width,
                _ => f32::MAX,
            };
            let _ = layout.SetMaxWidth(max_width);
            let _ = layout.SetTextAlignment(dwrite_alignment(alignment));
        }
        self.measure();
    }

    /// Reads size, lines, baseline and intrinsic widths back from DirectWrite.
    fn measure(&mut self) {
        let Some(layout) = &self.raw else {
            return;
        };
        // SAFETY: the layout is live and every out pointer refers to a local
        // that outlives its call.
        unsafe {
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_err() {
                return;
            }
            self.size = (metrics.widthIncludingTrailingWhitespace, metrics.height);
            self.is_rtl = offsets::first_strong_rtl(&self.text);

            let mut count = 0u32;
            let _ = layout.GetLineMetrics(None, &mut count);
            self.lines.clear();
            if count > 0 {
                let mut lines = vec![DWRITE_LINE_METRICS::default(); count as usize];
                if layout.GetLineMetrics(Some(&mut lines), &mut count).is_ok() {
                    let mut start = 0usize;
                    let mut y = 0.0f32;
                    for line in &lines[..count as usize] {
                        // `baseline` already includes the line's top offset, so
                        // it is absolute within the layout.
                        let end = start + line.length as usize;
                        self.lines.push(LineMetrics {
                            start: offsets::to_utf8(&self.text, start as u32),
                            end: offsets::to_utf8(&self.text, end as u32),
                            y,
                            height: line.height,
                            baseline: line.baseline,
                        });
                        y += line.height;
                        start = end;
                    }
                    self.first_baseline = self.lines.first().map(|line| line.baseline);
                }
            }
            let min = layout.DetermineMinWidth().unwrap_or(0.0);
            self.content_widths = ContentWidths {
                min,
                max: self.size.0.max(min),
            };
        }
    }

    /// The layout's COM object, for the metrics and paint modules.
    pub(super) fn raw(&self) -> Option<&IDWriteTextLayout> {
        self.raw.as_ref()
    }

    /// The layout's width in logical pixels.
    pub fn width(&self) -> f32 {
        self.size.0
    }

    /// The layout's height in logical pixels.
    pub fn height(&self) -> f32 {
        self.size.1
    }

    /// Number of laid-out lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Whether the paragraph's first strong character runs right to left.
    pub fn is_rtl(&self) -> bool {
        self.is_rtl
    }

    /// Offset of the first line's baseline from the top.
    pub fn first_baseline(&self) -> Option<f32> {
        self.first_baseline
    }

    /// Intrinsic soft-wrap limits.
    pub fn content_widths(&self) -> ContentWidths {
        self.content_widths
    }

    /// Metrics of one line, counted from the top of the layout.
    pub fn line_metrics(&self, line: usize) -> Option<LineMetrics> {
        self.lines.get(line).copied()
    }

    /// The line containing `y`.
    pub fn line_range_from_point(&self, _x: f32, y: f32) -> Range<usize> {
        for line in &self.lines {
            if y >= line.y && y <= line.y + line.height {
                return line.start..line.end;
            }
        }
        self.lines.last().map_or(0..0, |line| line.start..line.end)
    }

    /// The line containing byte `index`.
    pub fn line_range_for_index(&self, index: usize) -> Range<usize> {
        for line in &self.lines {
            if index >= line.start && index <= line.end {
                return line.start..line.end;
            }
        }
        self.lines.last().map_or(0..0, |line| line.start..line.end)
    }
}

impl std::fmt::Debug for DirectWriteLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectWriteLayout")
            .field("text", &self.text)
            .field("size", &self.size)
            .field("lines", &self.lines.len())
            .finish()
    }
}
