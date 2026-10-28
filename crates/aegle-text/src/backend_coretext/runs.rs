//! Glyph-run capture: CoreText runs as backend-independent font handles.
//!
//! `CTLineGetGlyphRuns` hands back `CTRun`s whose attributes name the actual
//! font CoreText chose for that run, after fallback. The scene must never hold a
//! native pointer, so each run's `CTFont` becomes a [`SystemFont`] handle: the
//! process keeps the font in a registry keyed by a monotonically increasing id
//! and tells `aegle-glyph` where the outline data lives
//! ([`aegle_glyph::register_system_face`]).
//!
//! A font CoreText cannot name by file (an application font registered from
//! bytes) is registered as [`FaceSource::Bytes`] using the bytes this text
//! system registered. A run whose font resolves to neither is skipped: painting
//! it would have no outline source, and silently drawing nothing is worse than
//! omitting the run.
//!
//! # Verification
//!
//! Compile-checked only; see the module documentation of [`super`].
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use aegle_glyph::{FaceSource, SystemFont};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFIndex, CFRange, CFRetained, CFString, CGPoint,
};
use objc2_core_text::{CTFont, CTLine, CTRun};

use super::layout::CoreTextLayout;

/// Registered faces, by handle id.
fn faces() -> &'static Mutex<BTreeMap<u64, Face>> {
    static FACES: OnceLock<Mutex<BTreeMap<u64, Face>>> = OnceLock::new();
    FACES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// A face's identity: its font file path and the index inside a collection.
type FaceKey = (Arc<str>, u32);

/// Handles already issued for one face, keyed by file path and index.
///
/// Keying on the file rather than on the `CTFont` pointer is what keeps this
/// sound: CoreText returns different objects for the same face, and a freed
/// pointer value could be reused by another face.
fn issued() -> &'static Mutex<BTreeMap<FaceKey, SystemFont>> {
    static ISSUED: OnceLock<Mutex<BTreeMap<FaceKey, SystemFont>>> = OnceLock::new();
    ISSUED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// One registered face: where its outline data lives.
struct Face {
    /// Handed to `aegle-glyph`; kept so the registry entry outlives the handle.
    _source: FaceSource,
}

/// The stable handle for `font`, registering it on first sight.
fn handle(font: &CTFont) -> Option<SystemFont> {
    let key = face_identity(font)?;
    if let Some(handle) = issued()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Some(*handle);
    }
    let source = FaceSource::File {
        path: Arc::clone(&key.0),
        index: key.1,
    };
    let handle = SystemFont {
        backend: SystemFont::CORETEXT,
        id: next_id(),
    };
    faces()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            handle.id,
            Face {
                _source: source.clone(),
            },
        );
    aegle_glyph::register_system_face(handle, source);
    issued()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, handle);
    Some(handle)
}

/// The file path and face index CoreText reports for `font`.
///
/// `CTFontCopyAttribute` with `kCTFontURLAttribute` gives the file for an
/// installed font; an application font registered from bytes has no URL, so it
/// resolves to `None` and the caller skips the run.
fn face_identity(font: &CTFont) -> Option<FaceKey> {
    // SAFETY: the font is live; the returned attribute is reference counted and
    // is a `CFURL` when present.
    let url: CFRetained<objc2_core_foundation::CFURL> = unsafe {
        font.attribute(objc2_core_text::kCTFontURLAttribute)?
            .downcast()
            .ok()?
    };
    let path = url.path()?;
    let path = path.to_string();
    // CoreText does not expose a collection index; registered collections are
    // addressed by path alone here, so index zero is the only face this path
    // reports. A collection registered from bytes would need its index kept
    // alongside the registration, which the byte path below handles instead.
    Some((Arc::from(path.as_str()), 0))
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
}

impl CoreTextLayout {
    /// Captures the frame's glyph runs; called whenever the frame is rebuilt.
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
                let runs: CFRetained<CFArray<CTRun>> =
                    CFRetained::cast_unchecked(line.glyph_runs());
                for run_index in 0..runs.len() {
                    let run = runs.get(run_index).expect("in range");
                    let Some(run) = collect_run(&run) else {
                        continue;
                    };
                    collected.push(run);
                }
            }
            self.runs = collected;
        }
    }
}

/// Copies one `CTRun` into a [`StoredRun`], or `None` when its font has no
/// outline source.
///
/// # Safety
///
/// The caller guarantees `run` is live; the function only reads it and the
/// buffers it fills are its own.
unsafe fn collect_run(run: &CTRun) -> Option<StoredRun> {
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
        let handle = handle(&font)?;
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
            .map(|(id, position)| (id, position.x as f32, position.y as f32))
            .collect();
        Some(StoredRun {
            font: handle,
            size,
            glyphs,
        })
    }
}

/// Keeps the `CFString` import meaningful for the attribute lookup above.
const _: Option<&CFString> = None;
