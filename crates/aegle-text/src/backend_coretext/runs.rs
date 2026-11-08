//! Glyph-run capture: CoreText runs as backend-independent font handles.
//!
//! `CTLineGetGlyphRuns` hands back `CTRun`s whose attributes name the actual
//! font CoreText chose for that run, after fallback. The scene must never hold a
//! native pointer, so each run's `CTFont` becomes a [`SystemFont`] handle, one
//! per PostScript name, and `aegle-glyph`'s CoreGraphics rasterizer receives
//! that font ([`aegle_glyph::cg::register_font`]). Every face CoreText can
//! shape with thereby rasterizes, collection members, variable instances and
//! application fonts included.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use aegle_glyph::SystemFont;
use objc2_core_foundation::{
    CFArray, CFDictionary, CFIndex, CFRange, CFRetained, CFString, CGPoint,
};
use objc2_core_text::{CTFont, CTLine, CTRun};

use super::layout::CoreTextLayout;

/// Handles already issued, by PostScript name.
///
/// Keying on the name rather than on the `CTFont` pointer is what keeps this
/// sound: CoreText returns different objects for the same face, at every size,
/// and a freed pointer value could be reused by another face. CoreText treats
/// PostScript names as unique, and each named instance of a variable font has
/// its own.
fn issued() -> &'static Mutex<BTreeMap<String, SystemFont>> {
    static ISSUED: OnceLock<Mutex<BTreeMap<String, SystemFont>>> = OnceLock::new();
    ISSUED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The stable handle for `font`, registering it on first sight.
fn handle(font: &CTFont) -> SystemFont {
    // SAFETY: the font is live; the name is reference counted.
    let name = unsafe { font.post_script_name() }.to_string();
    let mut issued = issued().lock().unwrap_or_else(PoisonError::into_inner);
    *issued.entry(name).or_insert_with(|| {
        let handle = SystemFont {
            backend: SystemFont::CORETEXT,
            id: next_id(),
        };
        aegle_glyph::cg::register_font(handle, font);
        handle
    })
}

/// One captured run: font handle, size and positioned glyph indices.
///
/// Kept independent of `aegle-scene` so diagnostics can count missing glyphs
/// even in builds without the scene feature; `paint.rs` converts it.
pub(super) struct StoredRun {
    /// The font handle, read by the scene bridge and by no other build.
    #[cfg_attr(not(feature = "scene"), allow(dead_code))]
    pub font: SystemFont,
    /// Logical font size of the run.
    #[cfg_attr(not(feature = "scene"), allow(dead_code))]
    pub size: f32,
    /// Glyph index, baseline x and baseline y in layout coordinates.
    pub glyphs: Vec<(u16, f32, f32)>,
    /// Whether CoreText fell back to `LastResort`, whose glyphs are the
    /// placeholders it draws for characters no font covers.
    pub last_resort: bool,
}

impl CoreTextLayout {
    /// Captures the frame's glyph runs; called whenever the frame is rebuilt,
    /// after `measure`, whose line origins and baselines place them.
    pub(super) fn capture_runs(&mut self) {
        self.runs.clear();
        let Some(frame) = self.frame() else {
            return;
        };
        // SAFETY: the frame is live; the line and run arrays are reference
        // counted, and every element is a `CTLine`/`CTRun` for the duration of
        // the loop.
        unsafe {
            // SAFETY: `CTFrameGetLines` returns an array of `CTLine`, and each
            // line's `CTLineGetGlyphRuns` an array of `CTRun`.
            let lines: CFRetained<CFArray<CTLine>> = CFRetained::cast_unchecked(frame.lines());
            let mut collected = Vec::new();
            for line_index in 0..lines.len() {
                let line = lines.get(line_index).expect("in range");
                // Run positions are relative to the line's origin, y up.
                let origin = (
                    self.origins[line_index].0 as f32,
                    self.lines[line_index].baseline,
                );
                let runs: CFRetained<CFArray<CTRun>> =
                    CFRetained::cast_unchecked(line.glyph_runs());
                for run_index in 0..runs.len() {
                    let run = runs.get(run_index).expect("in range");
                    let Some(run) = collect_run(&run, origin) else {
                        continue;
                    };
                    collected.push(run);
                }
            }
            self.runs = collected;
        }
    }
}

/// Copies one `CTRun` of the line whose origin lies at `origin` in layout
/// coordinates into a [`StoredRun`], or `None` when it has no glyphs or no
/// font.
///
/// # Safety
///
/// The caller guarantees `run` is live; the function only reads it and the
/// buffers it fills are its own.
unsafe fn collect_run(run: &CTRun, origin: (f32, f32)) -> Option<StoredRun> {
    // SAFETY: the caller guarantees `run` is live; every call below only reads
    // it, and the attribute dictionary it returns is a `CFDictionary` of
    // `CFString` keys to `CFType` values, which is what the cast asserts.
    unsafe {
        let count = run.glyph_count();
        if count <= 0 {
            return None;
        }
        let count = count as usize;
        let attributes: CFRetained<CFDictionary<CFString, objc2_core_foundation::CFType>> =
            CFRetained::cast_unchecked(run.attributes());
        let font = attributes.get(objc2_core_text::kCTFontAttributeName)?;
        let font = font.downcast::<CTFont>().ok()?;
        let handle = handle(&font);
        let last_resort = font.post_script_name().to_string() == "LastResort";
        let size = font.size() as f32;

        let mut indices = vec![0u16; count];
        let mut positions = vec![CGPoint { x: 0.0, y: 0.0 }; count];
        let range = CFRange {
            location: 0,
            length: count as CFIndex,
        };
        // Both buffers hold exactly `count` entries, which is what the run
        // reported, and the range covers them.
        run.glyphs(
            range,
            std::ptr::NonNull::new(indices.as_mut_ptr()).expect("nonempty"),
        );
        run.positions(
            range,
            std::ptr::NonNull::new(positions.as_mut_ptr()).expect("nonempty"),
        );
        let glyphs = indices
            .into_iter()
            .zip(positions)
            .map(|(id, position)| {
                let x = origin.0 + position.x as f32;
                (id, x, origin.1 - position.y as f32)
            })
            .collect();
        Some(StoredRun {
            font: handle,
            size,
            glyphs,
            last_resort,
        })
    }
}
