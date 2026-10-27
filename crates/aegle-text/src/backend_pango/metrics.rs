//! Line metrics, hit testing, caret geometry and text-range queries.
#![allow(unsafe_code)]

use std::ops::Range;

use std::ffi::CString;
use std::os::raw::c_int;

use super::layout::PangoLayout;
use super::sys::{
    PANGO_GLYPH_EMPTY, PANGO_GLYPH_UNKNOWN_FLAG, PANGO_SCALE, PangoLogAttr, PangoRectangle, pango,
};
use crate::layout::{Affinity, BoundingBox, LineMetrics};
use crate::{ContentWidths, TextDiagnostics};

impl PangoLayout {
    pub(super) fn recompute_metrics_and_runs(&mut self) {
        let p = match pango() {
            Some(p) => p,
            None => return,
        };
        unsafe {
            self.apply_line_height(p);
            let mut w: c_int = 0;
            let mut h: c_int = 0;
            (p.layout_get_pixel_size)(self.raw, &mut w, &mut h);
            self.size = (w as f32, h as f32);

            let base = (p.layout_get_baseline)(self.raw);
            self.first_baseline = if base > 0 {
                Some(base as f32 / PANGO_SCALE as f32)
            } else {
                None
            };

            self.lines.clear();
            let iter = (p.layout_get_iter)(self.raw);
            if !iter.is_null() {
                loop {
                    let line = (p.layout_iter_get_line_readonly)(iter);
                    if line.is_null() {
                        break;
                    }
                    let mut ext = PangoRectangle {
                        x: 0,
                        y: 0,
                        width: 0,
                        height: 0,
                    };
                    (p.layout_line_get_extents)(line, std::ptr::null_mut(), &mut ext);
                    let baseline = (p.layout_iter_get_baseline)(iter) as f32 / PANGO_SCALE as f32;
                    let line_ref = &*line;
                    // Pango measures line extents from the baseline; the
                    // layout's own coordinates start at its top.
                    let height = ext.height as f32 / PANGO_SCALE as f32;
                    self.lines.push(LineMetrics {
                        start: line_ref.start_index as usize,
                        end: (line_ref.start_index + line_ref.length) as usize,
                        y: baseline + ext.y as f32 / PANGO_SCALE as f32,
                        height,
                        baseline,
                    });
                    if (p.layout_iter_next_line)(iter) == 0 {
                        break;
                    }
                }
                (p.layout_iter_free)(iter);
            }

            #[cfg(feature = "scene")]
            self.capture_runs();
        }
    }

    pub(super) fn compute_content_widths(&mut self) {
        let p = match pango() {
            Some(p) => p,
            None => return,
        };
        unsafe {
            let mut max_w: c_int = 0;
            let mut h: c_int = 0;
            (p.layout_get_size)(self.raw, &mut max_w, &mut h);
            let max = max_w as f32 / PANGO_SCALE as f32;

            (p.layout_set_width)(self.raw, 0);
            let mut min_w: c_int = 0;
            (p.layout_get_size)(self.raw, &mut min_w, &mut h);
            let min = min_w as f32 / PANGO_SCALE as f32;

            let prev_w = self
                .width
                .map(|w| (w as f64 * PANGO_SCALE).round() as c_int)
                .unwrap_or(-1);
            (p.layout_set_width)(self.raw, prev_w);

            self.content_widths = ContentWidths { min, max };
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> (usize, Affinity) {
        let p = match pango() {
            Some(p) => p,
            None => return (0, Affinity::Downstream),
        };
        unsafe {
            let mut index: c_int = 0;
            let mut trailing: c_int = 0;
            (p.layout_xy_to_index)(
                self.raw,
                (x as f64 * PANGO_SCALE).round() as c_int,
                (y as f64 * PANGO_SCALE).round() as c_int,
                &mut index,
                &mut trailing,
            );
            let byte_idx = self.boundary(index.max(0) as usize);
            match trailing > 0 {
                true => (self.after(byte_idx), Affinity::Upstream),
                false => (byte_idx, Affinity::Downstream),
            }
        }
    }

    pub fn caret_geometry(&self, index: usize, _affinity: Affinity, width: f32) -> BoundingBox {
        let p = match pango() {
            Some(p) => p,
            None => return BoundingBox::default(),
        };
        unsafe {
            let mut strong = PangoRectangle {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
            let mut weak = PangoRectangle {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
            (p.layout_get_cursor_pos)(self.raw, index as c_int, &mut strong, &mut weak);
            let x0 = strong.x as f32 / PANGO_SCALE as f32;
            let y0 = strong.y as f32 / PANGO_SCALE as f32;
            let h = strong.height as f32 / PANGO_SCALE as f32;
            BoundingBox {
                x0,
                y0,
                x1: x0 + width,
                y1: y0 + h,
            }
        }
    }

    pub fn move_cursor(
        &self,
        index: usize,
        _affinity: Affinity,
        forward: bool,
    ) -> (usize, Affinity) {
        let p = match pango() {
            Some(p) => p,
            None => return (index, Affinity::Downstream),
        };
        unsafe {
            let mut new_index: c_int = 0;
            let mut new_trailing: c_int = 0;
            (p.layout_move_cursor_visually)(
                self.raw,
                1,
                index as c_int,
                0,
                if forward { 1 } else { -1 },
                &mut new_index,
                &mut new_trailing,
            );
            if new_index < 0 {
                (index, Affinity::Downstream)
            } else {
                let byte_idx = self.boundary(new_index as usize);
                match new_trailing > 0 {
                    true => (self.after(byte_idx), Affinity::Upstream),
                    false => (byte_idx, Affinity::Downstream),
                }
            }
        }
    }

    pub fn range_boxes(&self, range: Range<usize>, mut visit: impl FnMut(BoundingBox)) {
        let p = match pango() {
            Some(p) => p,
            None => return,
        };
        if range.is_empty() {
            return;
        }
        unsafe {
            let mut current: Option<BoundingBox> = None;
            let bytes = self.text.as_bytes();
            let mut idx = range.start.min(bytes.len());
            let end = range.end.min(bytes.len());

            while idx < end {
                let mut pos = PangoRectangle {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                };
                (p.layout_index_to_pos)(self.raw, idx as c_int, &mut pos);
                let x0 = pos.x as f32 / PANGO_SCALE as f32;
                let y0 = pos.y as f32 / PANGO_SCALE as f32;
                let w = pos.width as f32 / PANGO_SCALE as f32;
                let h = pos.height as f32 / PANGO_SCALE as f32;
                let b = BoundingBox {
                    x0,
                    y0,
                    x1: x0 + w,
                    y1: y0 + h,
                };

                match current {
                    Some(ref mut cur) if (cur.y0 - b.y0).abs() < 1.0 => {
                        cur.x1 = b.x1;
                    }
                    Some(cur) => {
                        visit(cur);
                        current = Some(b);
                    }
                    None => {
                        current = Some(b);
                    }
                }

                // Advance by UTF-8 character length
                let ch_len = if bytes[idx] < 0x80 {
                    1
                } else if bytes[idx] < 0xe0 {
                    2
                } else if bytes[idx] < 0xf0 {
                    3
                } else {
                    4
                };
                idx += ch_len;
            }
            if let Some(cur) = current {
                visit(cur);
            }
        }
    }

    pub fn grapheme_range(&self, line_text: &str, caret: usize, previous: bool) -> Range<usize> {
        let p = match pango() {
            Some(p) => p,
            None => return caret..caret,
        };
        let c_text = CString::new(line_text).unwrap_or_default();
        let n_chars = line_text.chars().count();
        let mut attrs = Vec::with_capacity(n_chars + 1);
        attrs.resize(n_chars + 1, PangoLogAttr { raw: 0 });

        unsafe {
            (p.get_log_attrs)(
                c_text.as_ptr(),
                line_text.len() as c_int,
                -1,
                std::ptr::null_mut(),
                attrs.as_mut_ptr(),
                (n_chars + 1) as c_int,
            );
        }

        let mut breaks = Vec::new();
        let mut byte_offset = 0;
        for (i, ch) in line_text.chars().enumerate() {
            if attrs[i].is_cursor_position() {
                breaks.push(byte_offset);
            }
            byte_offset += ch.len_utf8();
        }
        breaks.push(byte_offset);

        let mut start = 0;
        for &end in breaks.iter().skip(1) {
            if end > caret || (previous && end == caret) {
                return start..end;
            }
            start = end;
        }
        caret..caret
    }

    pub fn diagnostics(&self, text: &str) -> TextDiagnostics {
        let p = match pango() {
            Some(p) => p,
            None => return TextDiagnostics::default(),
        };
        unsafe {
            let mut missing = 0;
            let mut shaped_bytes = 0;
            let iter = (p.layout_get_iter)(self.raw);
            if !iter.is_null() {
                loop {
                    let run = (p.layout_iter_get_run_readonly)(iter);
                    if !run.is_null() {
                        let item = (*run).item;
                        let glyph_str = (*run).glyphs;
                        let font = (*item).analysis.font;
                        if !font.is_null() {
                            shaped_bytes += (*item).length as usize;
                            if !glyph_str.is_null() {
                                let n = (*glyph_str).num_glyphs;
                                let g_ptr = (*glyph_str).glyphs;
                                for i in 0..n {
                                    let g = (*g_ptr.add(i as usize)).glyph;
                                    if g != PANGO_GLYPH_EMPTY
                                        && (g == 0 || g & PANGO_GLYPH_UNKNOWN_FLAG != 0)
                                    {
                                        missing += 1;
                                    }
                                }
                            }
                        }
                    }
                    if (p.layout_iter_next_run)(iter) == 0 {
                        break;
                    }
                }
                (p.layout_iter_free)(iter);
            }
            let non_break_bytes = text.bytes().filter(|&b| b != b'\n' && b != b'\r').count();
            TextDiagnostics {
                missing_glyphs: missing,
                unshaped_bytes: non_break_bytes.saturating_sub(shaped_bytes),
            }
        }
    }
}
