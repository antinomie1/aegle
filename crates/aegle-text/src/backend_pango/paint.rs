//! Scene recording: font capture and positioned glyph runs.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_int};
use std::sync::{Arc, Mutex, OnceLock};

use aegle_scene::{Color, Glyph, GlyphRun, Point, SceneBuilder};
use linebender_resource_handle::{Blob, FontData};

use super::layout::{PangoLayout, StoredRun};
use super::sys::{PANGO_GLYPH_EMPTY, PANGO_GLYPH_UNKNOWN_FLAG, PANGO_SCALE, PangoRectangle, pango};

/// Font files already read by path, shared by every layout in this process.
///
/// Runs refer to their font by file and face index, so a layout Pango re-shapes
/// with another face cannot pair glyph indices with stale bytes.
fn files() -> &'static Mutex<HashMap<(String, u32), FontData>> {
    static FILES: OnceLock<Mutex<HashMap<(String, u32), FontData>>> = OnceLock::new();
    FILES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn font_data(path: &str, index: u32) -> Option<FontData> {
    let mut files = files().lock().ok()?;
    if let Some(font) = files.get(&(path.to_owned(), index)) {
        return Some(font.clone());
    }
    let bytes = fs::read(path).ok()?;
    let font = FontData::new(Blob::new(Arc::new(bytes)), index);
    files.insert((path.to_owned(), index), font.clone());
    Some(font)
}

/// Releases the process-wide font files; live layouts re-read them when painted.
pub(super) fn release_fonts() {
    if let Ok(mut files) = files().lock() {
        files.clear();
    }
}

impl PangoLayout {
    /// Records the positioned glyphs each paint call copies into a scene.
    pub(super) fn capture_runs(&mut self) {
        let p = match pango() {
            Some(p) => p,
            None => return,
        };
        self.runs.clear();
        let fc_file = CString::new("file").unwrap();
        let fc_index = CString::new("index").unwrap();
        unsafe {
            let iter = (p.layout_get_iter)(self.raw);
            if iter.is_null() {
                return;
            }
            loop {
                let run = (p.layout_iter_get_run_readonly)(iter);
                if !run.is_null() {
                    let item = (*run).item;
                    let glyph_str = (*run).glyphs;
                    let font = (*item).analysis.font;
                    if !font.is_null() && !glyph_str.is_null() {
                        let pattern = (p.fc_font_get_pattern)(font);
                        if !pattern.is_null() {
                            let mut file: *mut c_char = std::ptr::null_mut();
                            let mut face: c_int = 0;
                            (p.fc_pattern_get_string)(pattern, fc_file.as_ptr(), 0, &mut file);
                            (p.fc_pattern_get_integer)(pattern, fc_index.as_ptr(), 0, &mut face);
                            if !file.is_null() {
                                let path = CStr::from_ptr(file).to_string_lossy().into_owned();
                                let baseline =
                                    (p.layout_iter_get_baseline)(iter) as f32 / PANGO_SCALE as f32;
                                let mut extents = PangoRectangle {
                                    x: 0,
                                    y: 0,
                                    width: 0,
                                    height: 0,
                                };
                                (p.layout_iter_get_run_extents)(
                                    iter,
                                    std::ptr::null_mut(),
                                    &mut extents,
                                );
                                let mut x = extents.x as f32 / PANGO_SCALE as f32;
                                let glyphs = glyph_str_of(p, glyph_str, &mut x, baseline);
                                self.runs.push(StoredRun {
                                    path,
                                    index: face.max(0) as u32,
                                    size: self.font_size,
                                    glyphs,
                                });
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
    }
}

/// Copies one Pango run's positioned glyphs, skipping what Pango draws itself.
fn glyph_str_of(
    p: &super::sys::PangoLib,
    string: *mut super::sys::PangoGlyphString,
    x: &mut f32,
    baseline: f32,
) -> Vec<Glyph> {
    let _ = p;
    let mut glyphs = Vec::new();
    unsafe {
        let count = (*string).num_glyphs;
        let first = (*string).glyphs;
        for index in 0..count {
            let glyph = &*first.add(index as usize);
            let origin = Point::new(
                *x + glyph.geometry.x_offset as f32 / PANGO_SCALE as f32,
                baseline + glyph.geometry.y_offset as f32 / PANGO_SCALE as f32,
            );
            *x += glyph.geometry.width as f32 / PANGO_SCALE as f32;
            // Pango marks zero-width joins empty and text no font maps unknown;
            // the latter keeps its advance as a box Pango draws, not a glyph.
            if glyph.glyph == PANGO_GLYPH_EMPTY || glyph.glyph & PANGO_GLYPH_UNKNOWN_FLAG != 0 {
                continue;
            }
            let Ok(id) = u16::try_from(glyph.glyph) else {
                continue;
            };
            glyphs.push(Glyph {
                id,
                position: origin,
            });
        }
    }
    glyphs
}

impl PangoLayout {
    pub fn paint(&self, builder: &mut SceneBuilder, color: Color, weight: f32, origin: Point) {
        let bold = weight >= 600.0;
        for run in &self.runs {
            let Some(font) = font_data(&run.path, run.index) else {
                continue;
            };
            let glyphs = match origin == Point::default() {
                true => run.glyphs.clone(),
                false => run
                    .glyphs
                    .iter()
                    .map(|glyph| Glyph {
                        id: glyph.id,
                        position: Point::new(
                            origin.x + glyph.position.x,
                            origin.y + glyph.position.y,
                        ),
                    })
                    .collect(),
            };
            builder.glyphs(
                GlyphRun::new(font, run.size, color, Vec::new(), glyphs).synthesized(bold, 0),
            );
        }
    }
}
