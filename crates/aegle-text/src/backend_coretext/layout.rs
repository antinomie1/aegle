//! Retained CoreText layout: shaping, line breaking and measured geometry.
//!
//! One `CoreTextLayout` owns an attributed string and the `CTFrame` created
//! from it. Reflow rebuilds the frame for a new width, because CoreText line
//! breaking is a property of the frame's path; alignment and line height are
//! paragraph-style attributes of the attributed string. Line metrics, hit
//! testing, caret geometry and painting all read the same frame, so they cannot
//! disagree about where a character is.
#![allow(unsafe_code)]

use std::ops::Range;

use objc2_core_foundation::{
    CFArray, CFAttributedString, CFDictionary, CFRetained, CGPoint, CGRect,
};
use objc2_core_graphics::{CGMutablePath, CGPath};
use objc2_core_text::{CTFrame, CTFramesetter, CTLine};

use super::sys;
use super::system::CoreTextSystem;
use crate::layout::LineMetrics;
use crate::offsets;
use crate::style::Shaping;
use crate::{Alignment, ContentWidths};

/// A retained CoreText frame and the geometry measured from it.
pub(crate) struct CoreTextLayout {
    /// The attributed string the frame refers to.
    attributed: Option<CFRetained<CFAttributedString>>,
    /// The frame; `None` for an empty layout or when CoreText failed.
    frame: Option<CFRetained<CTFrame>>,
    /// UTF-8 text; CoreText indexes it in UTF-16 code units.
    pub(super) text: String,
    pub(super) width: Option<f32>,
    pub(super) wrap: bool,
    pub(super) size: (f32, f32),
    pub(super) lines: Vec<LineMetrics>,
    pub(super) first_baseline: Option<f32>,
    pub(super) content_widths: ContentWidths,
    pub(super) is_rtl: bool,
    /// Alignment the frame was built with, kept for rebuilds.
    pub(super) alignment: Alignment,
    /// Glyph runs captured for painting and diagnostics.
    pub(super) runs: Vec<super::runs::StoredRun>,
    /// Font size and line height, kept to rebuild the attributed string.
    font_size: f32,
    line_height: crate::LineHeight,
    /// Shaping key the attributed string was built with, kept for rebuilds.
    shaping: Option<Shaping>,
    /// Underline range, kept for rebuilds.
    underline: Option<Range<usize>>,
    /// Each line's origin in the frame's bottom-left coordinate space.
    pub(super) origins: Vec<(f64, f64)>,
    /// The frame's total height, for converting to Aegle's top-down space.
    frame_height: f64,
}

impl CoreTextLayout {
    /// Shapes `text` with `shaping` and lays it out without soft wrapping.
    pub fn new(
        system: &mut CoreTextSystem,
        text: &str,
        shaping: &Shaping,
        wrap: bool,
        underline: Option<Range<usize>>,
    ) -> Self {
        let mut layout = Self::empty();
        layout.text = text.to_owned();
        layout.wrap = wrap;
        layout.font_size = shaping.size;
        layout.line_height = shaping.line_height;
        layout.shaping = Some(shaping.clone());
        layout.underline = underline.clone();
        layout.build(system, shaping, underline);
        layout
    }

    /// An empty layout, used for released paragraphs.
    pub fn empty() -> Self {
        Self {
            attributed: None,
            frame: None,
            text: String::new(),
            width: None,
            wrap: false,
            size: (0.0, 0.0),
            lines: Vec::new(),
            first_baseline: None,
            content_widths: ContentWidths::default(),
            is_rtl: false,
            alignment: Alignment::Start,
            runs: Vec::new(),
            font_size: 0.0,
            line_height: crate::LineHeight::default(),
            shaping: None,
            underline: None,
            origins: Vec::new(),
            frame_height: 0.0,
        }
    }

    /// Builds the attributed string and its frame for `shaping`.
    fn build(
        &mut self,
        system: &CoreTextSystem,
        shaping: &Shaping,
        underline: Option<Range<usize>>,
    ) {
        self.attributed = None;
        self.frame = None;
        self.shaping = Some(shaping.clone());
        self.underline = underline;
        if system.font(shaping).is_none() {
            return;
        }
        self.build_attributed();
        self.build_frame();
        self.measure();
        self.capture_runs();
    }

    /// Builds the attributed string from the retained shaping key.
    ///
    /// The font, paragraph style (alignment and line height) and attributes all
    /// live here, so a reflow that only changes the width can reuse the string
    /// while a changed alignment or line height rebuilds it.
    fn build_attributed(&mut self) {
        let Some(shaping) = self.shaping.clone() else {
            return;
        };
        self.attributed = None;
        self.frame = None;
        let Some(font) = super::fonts::font(
            &shaping.families,
            f64::from(self.font_size),
            shaping.weight,
            shaping.slant,
        ) else {
            return;
        };
        let natural = natural_line_height(&font);
        let target = offsets::line_height(self.line_height, natural, self.font_size).map(f64::from);
        let style = sys::paragraph_style(self.alignment, target);
        let attributes = sys::text_attributes(&font, &style, self.underline.is_some());
        // SAFETY: the string, attributes and font are live for the call;
        // CoreFoundation copies what the attributed string needs, and the result
        // is reference counted.
        self.attributed = unsafe {
            let text = sys::cf_string(&self.text);
            let attributes: CFRetained<CFDictionary> = CFRetained::cast_unchecked(attributes);
            CFAttributedString::new(None, Some(&text), Some(&attributes))
        };
    }

    /// Creates the frame for the current width.
    fn build_frame(&mut self) {
        let Some(attributed) = &self.attributed else {
            return;
        };
        // SAFETY: the attributed string is live and owned; the framesetter and
        // frame are reference counted.
        let frame = unsafe {
            let framesetter = CTFramesetter::with_attributed_string(attributed);
            let path = frame_path(self.width, self.text.len());
            framesetter.frame(CFRangeFor::ALL, &path, None)
        };
        self.frame = Some(frame);
    }

    /// Applies wrapping width and alignment.
    ///
    /// CoreText line breaking belongs to the frame's path, so a width change
    /// rebuilds the frame. Alignment is a paragraph-style attribute of the
    /// attributed string, so a changed alignment rebuilds the string too. A
    /// rebuild re-shapes, which is what the other backends also do when their
    /// paragraph style changes; a reflow to the same constraints does nothing.
    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) {
        let width_changed = self.width != width;
        let alignment_changed = self.alignment != alignment;
        self.width = width;
        self.alignment = alignment;
        if !width_changed && !alignment_changed {
            return;
        }
        if self.attributed.is_none() {
            return;
        }
        match (alignment_changed, self.shaping.clone()) {
            (true, Some(shaping)) => {
                // Rebuild the attributed string so the paragraph style carries
                // the new alignment.
                let mut rebuilt = Self::empty();
                rebuilt.text = self.text.clone();
                rebuilt.width = self.width;
                rebuilt.wrap = self.wrap;
                rebuilt.alignment = self.alignment;
                rebuilt.font_size = self.font_size;
                rebuilt.line_height = self.line_height;
                rebuilt.shaping = Some(shaping.clone());
                rebuilt.underline = self.underline.clone();
                rebuilt.build_attributed();
                rebuilt.build_frame();
                rebuilt.measure();
                rebuilt.capture_runs();
                *self = rebuilt;
            }
            _ => {
                self.build_frame();
                self.measure();
                self.capture_runs();
            }
        }
    }

    /// Reads size, lines, baseline and intrinsic widths from the frame.
    fn measure(&mut self) {
        let Some(frame) = &self.frame else {
            return;
        };
        // SAFETY: the frame is live and every out pointer refers to a local
        // that outlives its call.
        unsafe {
            self.is_rtl = offsets::first_strong_rtl(&self.text);
            // SAFETY: `CTFrameGetLines` always returns an array of `CTLine`.
            let lines: CFRetained<CFArray<CTLine>> = CFRetained::cast_unchecked(frame.lines());
            let count = lines.len();
            self.lines.clear();
            let mut origins = vec![CGPoint { x: 0.0, y: 0.0 }; count];
            if count > 0 {
                frame.line_origins(
                    CFRangeFor::ALL,
                    std::ptr::NonNull::new(origins.as_mut_ptr()).expect("nonempty"),
                );
            }
            // CoreText's frame origin is bottom left; Aegle's is top left, so
            // every line's y is flipped against the frame's total height.
            let mut total = 0.0f64;
            let mut measured = Vec::with_capacity(count);
            self.origins.clear();
            for origin in origins.iter().take(count) {
                self.origins.push((origin.x, origin.y));
            }
            for (index, origin) in origins.iter().enumerate().take(count) {
                let line = lines.get(index).expect("in range");
                let mut ascent = 0.0f64;
                let mut descent = 0.0f64;
                let mut leading = 0.0f64;
                let width = line.typographic_bounds(&mut ascent, &mut descent, &mut leading);
                let range = line.string_range();
                let start = offsets::to_utf8(&self.text, range.location.max(0) as u32);
                let end =
                    offsets::to_utf8(&self.text, (range.location + range.length).max(0) as u32);
                let height = ascent + descent + leading;
                measured.push((start, end, height, ascent, width, origin.y));
                total = total.max(origin.y + height);
            }
            for (start, end, height, ascent, _width, origin_y) in measured {
                let top = total - origin_y - height;
                self.lines.push(LineMetrics {
                    start,
                    end,
                    y: top as f32,
                    height: height as f32,
                    baseline: (top + ascent) as f32,
                });
            }
            self.first_baseline = self.lines.first().map(|line| line.baseline);
            self.frame_height = total;
            self.size = (
                self.measure_width(),
                self.lines.iter().map(|line| line.height).sum(),
            );
            self.content_widths = ContentWidths {
                min: self.measure_min_width(),
                max: self.size.0,
            };
        }
    }

    /// The widest line's typographic width.
    fn measure_width(&self) -> f32 {
        let Some(frame) = &self.frame else {
            return 0.0;
        };
        // SAFETY: the frame is live; the line array is reference counted.
        unsafe {
            // SAFETY: `CTFrameGetLines` always returns an array of `CTLine`.
            let lines: CFRetained<CFArray<CTLine>> = CFRetained::cast_unchecked(frame.lines());
            let mut widest = 0.0f64;
            for index in 0..lines.len() {
                let line = lines.get(index).expect("in range");
                let width = line.typographic_bounds(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                widest = widest.max(width);
            }
            widest as f32
        }
    }

    /// The minimum width CoreText can break to, as the narrowest word width.
    ///
    /// CoreText exposes no `DetermineMinWidth`, so this takes the narrowest
    /// line width obtained by laying the text out at the smallest width that
    /// still produces one line per word: the framesetter's suggested size at a
    /// zero-width constraint. When that fails, the current width stands in.
    fn measure_min_width(&self) -> f32 {
        let Some(attributed) = &self.attributed else {
            return 0.0;
        };
        // SAFETY: the attributed string is live; the suggested size call only
        // reads it, and the returned framesetter is reference counted.
        unsafe {
            let framesetter = CTFramesetter::with_attributed_string(attributed);
            let size = framesetter.suggest_frame_size_with_constraints(
                CFRangeFor::ALL,
                None,
                objc2_core_foundation::CGSize {
                    width: 0.0,
                    height: f64::MAX,
                },
                std::ptr::null_mut(),
            );
            if size.width.is_finite() && size.width > 0.0 {
                size.width as f32
            } else {
                self.size.0
            }
        }
    }

    /// The frame's COM-equivalent object, for the metrics and paint modules.
    pub(super) fn frame(&self) -> Option<&CTFrame> {
        self.frame.as_deref()
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

impl std::fmt::Debug for CoreTextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreTextLayout")
            .field("text", &self.text)
            .field("size", &self.size)
            .field("lines", &self.lines.len())
            .finish()
    }
}

/// A rectangle path covering `width` by an effectively unbounded height.
///
/// A width of `None` means "no soft wrapping": the path is as wide as the text
/// can be, which CoreText treats as a single line.
fn frame_path(width: Option<f32>, text_len: usize) -> CFRetained<CGPath> {
    let wide = match width {
        Some(width) => f64::from(width).max(0.0),
        // One em per byte is a safe upper bound for any script.
        None => (text_len as f64 + 1.0) * 64.0,
    };
    let path = CGMutablePath::new();
    // SAFETY: the path is live and owned; adding a rectangle only mutates it.
    unsafe {
        CGMutablePath::add_rect(
            Some(&path),
            std::ptr::null(),
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: objc2_core_foundation::CGSize {
                    width: wide,
                    height: 16_777_216.0,
                },
            },
        );
    }
    // SAFETY: a mutable path is a `CGPath` at a less specific type.
    unsafe { CFRetained::cast_unchecked(path) }
}

/// The natural line height of `font`, in points.
fn natural_line_height(font: &objc2_core_text::CTFont) -> f32 {
    // SAFETY: the font is live; the getters only read metrics.
    unsafe {
        let ascent = font.ascent();
        let descent = font.descent();
        let leading = font.leading();
        (ascent + descent + leading) as f32
    }
}

/// `CFRange` covering everything; CoreText treats a zero length as "to the end".
struct CFRangeFor;

impl CFRangeFor {
    /// The range `(0, 0)`, meaning the whole string for the APIs used here.
    const ALL: objc2_core_foundation::CFRange = objc2_core_foundation::CFRange {
        location: 0,
        length: 0,
    };
}
