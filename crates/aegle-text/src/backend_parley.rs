//! Fallback Parley layout backend (unmaintained).
#![cfg(any(feature = "text-parley", not(target_os = "linux")))]

use std::ops::Range;

#[cfg(feature = "scene")]
use aegle_scene::{Glyph, GlyphRun, SceneBuilder};
use aegle_types::Color;
#[cfg(feature = "scene")]
use aegle_types::Point;
#[cfg(feature = "scene")]
use parley::PositionedLayoutItem;
use parley::{Affinity as ParleyAffinity, AlignmentOptions, Cursor, Layout as RawLayout};

use crate::{
    Alignment, ContentWidths, TextDiagnostics,
    layout::{Affinity, BoundingBox, LineMetrics},
};

#[derive(Debug)]
pub(crate) struct ParleyLayout {
    pub(crate) raw: RawLayout<Color>,
}

impl ParleyLayout {
    /// An empty layout the caller shapes into.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            raw: RawLayout::new(),
        }
    }

    pub fn reflow(&mut self, width: Option<f32>, alignment: Alignment) {
        self.raw.break_all_lines(width);
        self.raw
            .align(alignment.parley(), AlignmentOptions::default());
    }

    pub fn width(&self) -> f32 {
        self.raw.width()
    }

    pub fn height(&self) -> f32 {
        self.raw.height()
    }

    pub fn line_count(&self) -> usize {
        self.raw.len()
    }

    pub fn is_rtl(&self) -> bool {
        self.raw.is_rtl()
    }

    pub fn first_baseline(&self) -> Option<f32> {
        Some(self.raw.lines().next()?.metrics().baseline)
    }

    pub fn content_widths(&self) -> ContentWidths {
        let cw = self.raw.calculate_content_widths();
        ContentWidths {
            min: cw.min,
            max: cw.max,
        }
    }

    pub fn line_metrics(&self, line: usize) -> Option<LineMetrics> {
        let l = self.raw.get(line)?;
        let m = l.metrics();
        Some(LineMetrics {
            start: 0,
            end: 0,
            y: m.block_max_coord - m.line_height,
            height: m.line_height,
            baseline: m.baseline,
        })
    }

    pub fn line_range_from_point(&self, x: f32, y: f32) -> Range<usize> {
        let sel = parley::Selection::line_from_point(&self.raw, x, y);
        sel.text_range()
    }

    pub fn line_range_for_index(&self, index: usize) -> Range<usize> {
        let cursor = Cursor::from_byte_index(&self.raw, index, ParleyAffinity::Downstream);
        let sel = parley::Selection::from(cursor);
        let start = sel.line_start(&self.raw, false).focus().index();
        let end = sel.line_end(&self.raw, false).focus().index();
        start..end
    }

    pub fn hit_test(&self, x: f32, y: f32) -> (usize, Affinity) {
        let cursor = Cursor::from_point(&self.raw, x, y);
        (
            cursor.index(),
            match cursor.affinity() {
                ParleyAffinity::Downstream => Affinity::Downstream,
                ParleyAffinity::Upstream => Affinity::Upstream,
            },
        )
    }

    pub fn caret_geometry(&self, index: usize, affinity: Affinity, width: f32) -> BoundingBox {
        let parley_aff = match affinity {
            Affinity::Downstream => ParleyAffinity::Downstream,
            Affinity::Upstream => ParleyAffinity::Upstream,
        };
        let cursor = Cursor::from_byte_index(&self.raw, index, parley_aff);
        let bbox = cursor.geometry(&self.raw, width);
        BoundingBox {
            x0: bbox.x0 as f32,
            y0: bbox.y0 as f32,
            x1: bbox.x1 as f32,
            y1: bbox.y1 as f32,
        }
    }

    pub fn move_cursor(
        &self,
        index: usize,
        affinity: Affinity,
        forward: bool,
    ) -> (usize, Affinity) {
        let parley_aff = match affinity {
            Affinity::Downstream => ParleyAffinity::Downstream,
            Affinity::Upstream => ParleyAffinity::Upstream,
        };
        let cursor = Cursor::from_byte_index(&self.raw, index, parley_aff);
        let next = match forward {
            true => cursor.next_visual(&self.raw),
            false => cursor.previous_visual(&self.raw),
        };
        (
            next.index(),
            match next.affinity() {
                ParleyAffinity::Downstream => Affinity::Downstream,
                ParleyAffinity::Upstream => Affinity::Upstream,
            },
        )
    }

    pub fn range_boxes(&self, range: Range<usize>, mut visit: impl FnMut(BoundingBox)) {
        if range.is_empty() {
            return;
        }
        let selection = parley::Selection::new(
            Cursor::from_byte_index(&self.raw, range.start, ParleyAffinity::Downstream),
            Cursor::from_byte_index(&self.raw, range.end, ParleyAffinity::Upstream),
        );
        selection.geometry_with(&self.raw, |rect, _| {
            visit(BoundingBox {
                x0: rect.x0 as f32,
                y0: rect.y0 as f32,
                x1: rect.x1 as f32,
                y1: rect.y1 as f32,
            });
        });
    }

    pub fn grapheme_range(&self, line_text: &str, caret: usize, previous: bool) -> Range<usize> {
        let mut start = 0;
        for end in icu_segmenter::GraphemeClusterSegmenter::new()
            .segment_str(line_text)
            .skip(1)
        {
            if end > caret || (previous && end == caret) {
                return start..end;
            }
            start = end;
        }
        caret..caret
    }

    pub fn diagnostics(&self, text: &str) -> TextDiagnostics {
        let mut shaped_bytes = 0;
        let mut missing_glyphs = 0;
        for line in self.raw.lines() {
            for run in line.runs() {
                for cluster in run.clusters() {
                    let range = cluster.text_range();
                    if text.is_empty() {
                        continue;
                    }
                    shaped_bytes += range.len();
                    if !text[range].chars().all(char::is_control) {
                        missing_glyphs += cluster.glyphs().filter(|glyph| glyph.id == 0).count();
                    }
                }
            }
        }
        TextDiagnostics {
            missing_glyphs,
            unshaped_bytes: text.len().saturating_sub(shaped_bytes),
        }
    }

    #[cfg(feature = "scene")]
    pub fn paint(&self, builder: &mut SceneBuilder, color: Color, weight: f32, origin: Point) {
        let bold = weight >= 600.0;
        for line in self.raw.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(positioned) = item else {
                    continue;
                };
                let run = positioned.run();
                let glyphs = positioned
                    .positioned_glyphs()
                    .map(|glyph| Glyph {
                        id: glyph.id as u16,
                        position: Point::new(origin.x + glyph.x, origin.y + glyph.y),
                    })
                    .collect();
                let run = GlyphRun::new(
                    run.font().clone(),
                    run.font_size(),
                    color,
                    run.normalized_coords().to_vec(),
                    glyphs,
                )
                .synthesized(
                    bold && run.synthesis().embolden(),
                    run.synthesis().skew().map_or(0, |a| a.round() as i8),
                );
                builder.glyphs(run);
            }
        }
    }
}
