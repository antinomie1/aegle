//! Retained Pango layout: shaping, line breaking and its measured geometry.
#![allow(unsafe_code)]

use std::ffi::CString;
use std::ops::Range;
use std::os::raw::{c_int, c_void};

#[cfg(feature = "scene")]
use aegle_scene::Glyph;

use super::sys::{
    PANGO_ALIGN_CENTER, PANGO_ALIGN_LEFT, PANGO_ALIGN_RIGHT, PANGO_SCALE, PANGO_UNDERLINE_SINGLE,
    PANGO_WRAP_WORD_CHAR, PangoLib, PangoRectangle, pango,
};
use super::system::PangoSystem;
use crate::layout::LineMetrics;
use crate::style::Shaping;
use crate::{Alignment, ContentWidths};

#[cfg(feature = "scene")]
pub(super) struct StoredRun {
    /// Font file the run's glyph indices belong to, and its face index.
    pub(super) path: String,
    pub(super) index: u32,
    pub(super) size: f32,
    pub(super) glyphs: Vec<Glyph>,
}

pub(crate) struct PangoLayout {
    pub(super) raw: *mut c_void,
    pub(super) text: String,
    pub(super) width: Option<f32>,
    pub(super) wrap: bool,
    pub(super) font_size: f32,
    pub(super) line_height: crate::LineHeight,
    pub(super) alignment: Alignment,
    pub(super) lines: Vec<LineMetrics>,
    pub(super) size: (f32, f32),
    pub(super) first_baseline: Option<f32>,
    pub(super) content_widths: ContentWidths,
    pub(super) is_rtl: bool,
    #[cfg(feature = "scene")]
    pub(super) runs: Vec<StoredRun>,
}

impl PangoLayout {
    pub fn new(
        system: &mut PangoSystem,
        text: &str,
        shaping: &Shaping,
        wrap: bool,
        underline: Option<Range<usize>>,
    ) -> Self {
        let p = pango().expect("Pango is available on this system");
        unsafe {
            let raw = (p.layout_new)(system.context);
            let c_text = CString::new(text).unwrap_or_default();
            (p.layout_set_text)(raw, c_text.as_ptr(), text.len() as c_int);

            let desc = (p.font_description_new)();
            let families = system
                .resolve_families(&shaping.families, shaping.locale.as_deref())
                .unwrap_or_default();
            (p.font_description_set_family)(desc, families.as_ptr());
            (p.font_description_set_absolute_size)(desc, (shaping.size as f64) * PANGO_SCALE);
            (p.font_description_set_weight)(desc, shaping.weight.round() as c_int);
            let style = match shaping.slant {
                crate::Slant::Normal => 0,
                crate::Slant::Oblique(_) => 1,
                crate::Slant::Italic => 2,
            };
            (p.font_description_set_style)(desc, style);
            (p.layout_set_font_description)(raw, desc);
            (p.font_description_free)(desc);

            if wrap {
                (p.layout_set_wrap)(raw, PANGO_WRAP_WORD_CHAR);
            }

            let attrs = (p.attr_list_new)();
            if let Some(range) = underline.filter(|r| !r.is_empty()) {
                let attr = (p.attr_underline_new)(PANGO_UNDERLINE_SINGLE);
                let ptr = attr as *mut u32;
                *ptr.add(2) = range.start as u32;
                *ptr.add(3) = range.end as u32;
                (p.attr_list_insert)(attrs, attr);
            }
            if shaping.letter_spacing != 0.0 {
                let attr = (p.attr_letter_spacing_new)(
                    (shaping.letter_spacing as f64 * PANGO_SCALE).round() as c_int,
                );
                (p.attr_list_insert)(attrs, attr);
            }
            (p.layout_set_attributes)(raw, attrs);
            (p.attr_list_unref)(attrs);

            let mut layout = Self {
                raw,
                text: text.to_string(),
                width: None,
                wrap,
                font_size: shaping.size,
                line_height: shaping.line_height,
                alignment: Alignment::Start,
                lines: Vec::new(),
                size: (0.0, 0.0),
                first_baseline: None,
                content_widths: ContentWidths::default(),
                is_rtl: false,
                #[cfg(feature = "scene")]
                runs: Vec::new(),
            };
            layout.recompute_metrics_and_runs();
            layout.compute_content_widths();
            layout
        }
    }

    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) {
        let p = match pango() {
            Some(p) => p,
            None => return,
        };
        self.width = width;
        self.alignment = alignment;
        unsafe {
            let p_width = if self.wrap {
                width
                    .map(|w| (w as f64 * PANGO_SCALE).round() as c_int)
                    .unwrap_or(-1)
            } else {
                -1
            };
            (p.layout_set_width)(self.raw, p_width);
            let p_align = match alignment {
                Alignment::Start | Alignment::Left => PANGO_ALIGN_LEFT,
                Alignment::Center => PANGO_ALIGN_CENTER,
                Alignment::Right | Alignment::End | Alignment::Justify => PANGO_ALIGN_RIGHT,
            };
            (p.layout_set_alignment)(self.raw, p_align);
            self.recompute_metrics_and_runs();
        }
    }

    /// Applies Aegle's line height as Pango's extra-spacing factor.
    ///
    /// Pango's factor adds that fraction of each line's own height, so a target
    /// height becomes `extra / natural` after measuring the first line, whose
    /// metrics stand in for the paragraph's.
    pub(super) fn apply_line_height(&self, p: &PangoLib) {
        unsafe {
            let iter = (p.layout_get_iter)(self.raw);
            if iter.is_null() {
                return;
            }
            let mut logical = PangoRectangle {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
            (p.layout_iter_get_line_extents)(iter, std::ptr::null_mut(), &mut logical);
            (p.layout_iter_free)(iter);
            let natural = f64::from(logical.height) / PANGO_SCALE;
            if natural <= 0.0 {
                return;
            }
            let target = match self.line_height {
                crate::LineHeight::MetricsRelative(factor) => natural * f64::from(factor),
                crate::LineHeight::FontSizeRelative(factor) => {
                    f64::from(self.font_size) * f64::from(factor)
                }
                crate::LineHeight::Absolute(pixels) => f64::from(pixels),
            };
            // Pango multiplies each line's height by this factor; zero means natural.
            let factor = (target / natural).max(f64::EPSILON);
            (p.layout_set_line_spacing)(self.raw, factor as f32);
        }
    }

    pub fn width(&self) -> f32 {
        self.size.0
    }

    pub fn height(&self) -> f32 {
        self.size.1
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn is_rtl(&self) -> bool {
        self.is_rtl
    }

    pub fn first_baseline(&self) -> Option<f32> {
        self.first_baseline
    }

    pub fn content_widths(&self) -> ContentWidths {
        self.content_widths
    }

    pub fn line_metrics(&self, line: usize) -> Option<LineMetrics> {
        self.lines.get(line).copied()
    }

    pub fn line_range_from_point(&self, _x: f32, y: f32) -> Range<usize> {
        for line in &self.lines {
            if y >= line.y && y <= line.y + line.height {
                return line.start..line.end;
            }
        }
        self.lines.last().map(|l| l.start..l.end).unwrap_or(0..0)
    }

    pub fn line_range_for_index(&self, index: usize) -> Range<usize> {
        for line in &self.lines {
            if index >= line.start && index <= line.end {
                return line.start..line.end;
            }
        }
        self.lines.last().map(|l| l.start..l.end).unwrap_or(0..0)
    }

    /// Snaps a Pango byte offset down to a UTF-8 character boundary.
    pub(super) fn boundary(&self, index: usize) -> usize {
        let mut index = index.min(self.text.len());
        while index > 0 && !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    /// The byte offset after the character at `index`.
    pub(super) fn after(&self, index: usize) -> usize {
        let index = self.boundary(index);
        index + self.text[index..].chars().next().map_or(0, char::len_utf8)
    }
}

impl std::fmt::Debug for PangoLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PangoLayout")
            .field("text", &self.text)
            .field("size", &self.size)
            .field("lines", &self.lines.len())
            .finish()
    }
}

impl Drop for PangoLayout {
    fn drop(&mut self) {
        if let Some(p) = pango() {
            unsafe {
                (p.g_object_unref)(self.raw);
            }
        }
    }
}
