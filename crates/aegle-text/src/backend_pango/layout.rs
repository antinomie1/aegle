//! Retained Pango layout: shaping, line breaking and its measured geometry.
#![allow(unsafe_code)]

use std::ffi::CString;
use std::ops::Range;
use std::os::raw::{c_int, c_void};

#[cfg(feature = "scene")]
use aegle_scene::Glyph;

use super::sys::{
    PANGO_ALIGN_CENTER, PANGO_ALIGN_LEFT, PANGO_ALIGN_RIGHT, PANGO_SCALE, PANGO_UNDERLINE_SINGLE,
    PANGO_WRAP_WORD_CHAR, PangoLib, pango,
};
use super::system::PangoSystem;
use crate::layout::LineMetrics;
use crate::style::Shaping;
use crate::{Alignment, ContentWidths, LineHeight};

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
    /// Pixel size the captured glyph runs are drawn at.
    #[cfg(feature = "scene")]
    pub(super) font_size: f32,
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
            if let Some(attr) = line_height_attr(p, shaping.line_height, shaping.size) {
                (p.attr_list_insert)(attrs, attr);
            }
            (p.layout_set_attributes)(raw, attrs);
            (p.attr_list_unref)(attrs);

            let mut layout = Self {
                raw,
                text: text.to_string(),
                width: None,
                wrap,
                #[cfg(feature = "scene")]
                font_size: shaping.size,
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

/// The attribute that makes every line box `line_height` tall, the first too.
///
/// Pango's line-height attribute follows CSS: the difference from the font's
/// own line is split evenly above and below, so a single-line label gets the
/// requested box. (`pango_layout_set_line_spacing` only spaces baselines apart
/// and leaves one line at its natural height.) The font's own height needs no
/// attribute, and the style boundary has already rejected unusable values.
fn line_height_attr(p: &PangoLib, line_height: LineHeight, font_size: f32) -> Option<*mut c_void> {
    let pixels = match line_height {
        LineHeight::MetricsRelative(1.0) => return None,
        // SAFETY: the constructor returns a new attribute the caller inserts.
        LineHeight::MetricsRelative(factor) => {
            return Some(unsafe { (p.attr_line_height_new)(f64::from(factor)) });
        }
        LineHeight::FontSizeRelative(factor) => font_size * factor,
        LineHeight::Absolute(pixels) => pixels,
    };
    let units = (f64::from(pixels) * PANGO_SCALE).round() as c_int;
    // SAFETY: as above.
    Some(unsafe { (p.attr_line_height_new_absolute)(units) })
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
