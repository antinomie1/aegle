//! Glyph-run capture: DirectWrite faces as backend-independent font handles.
//!
//! DirectWrite's `Draw` calls back once per glyph run with an
//! `IDWriteFontFace`. The scene must never hold that COM pointer, so each face
//! becomes a [`SystemFont`] handle: the process keeps the face in a registry
//! keyed by a monotonically increasing id and tells `aegle-glyph` where its
//! outline data lives ([`aegle_glyph::register_system_face`]).
//!
//! Faces of application fonts have no file DirectWrite can name, so their
//! bytes come from the text system's registrations and are looked up by
//! matching the face's file path against them. A face that matches nothing is
//! skipped: painting it would have no outline source, and silently drawing
//! nothing is worse than omitting the run from the scene.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use aegle_glyph::{FaceSource, SystemFont};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_GLYPH_RUN, DWRITE_GLYPH_RUN_DESCRIPTION, DWRITE_MATRIX, DWRITE_MEASURING_MODE,
    IDWriteFontFace, IDWriteInlineObject, IDWritePixelSnapping_Impl, IDWriteTextRenderer,
    IDWriteTextRenderer_Impl,
};
use windows::core::BOOL;
use windows::core::{Interface, Ref, Result, implement};

use super::layout::DirectWriteLayout;

/// Registered faces, by handle id.
fn faces() -> &'static Mutex<BTreeMap<u64, Face>> {
    static FACES: OnceLock<Mutex<BTreeMap<u64, Face>>> = OnceLock::new();
    FACES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// A face's identity: its font file path and the index inside a collection.
type FaceKey = (Arc<str>, u32);

/// Handles already issued for one face, keyed by the face's file and index.
///
/// Keying on `(path, index)` rather than on the COM pointer is what keeps this
/// sound: DirectWrite may hand out a different pointer for the same face on
/// every `Draw`, and a freed pointer value could be reused by another face.
fn issued() -> &'static Mutex<BTreeMap<FaceKey, SystemFont>> {
    static ISSUED: OnceLock<Mutex<BTreeMap<FaceKey, SystemFont>>> = OnceLock::new();
    ISSUED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// One registered face.
struct Face {
    /// The outline source handed to `aegle-glyph`; kept so the registry entry
    /// outlives the handle and a later trim can find it.
    _source: FaceSource,
}

/// The stable handle for `face`, registering it on first sight.
fn handle(face: &IDWriteFontFace) -> Option<SystemFont> {
    let (path, index) = face_identity(face)?;
    let key = (Arc::from(path.as_str()), index);
    if let Some(font) = issued()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Some(*font);
    }
    let source = FaceSource::File {
        path: Arc::clone(&key.0),
        index,
    };
    let font = SystemFont {
        backend: SystemFont::DWRITE,
        id: next_id(),
    };
    faces()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            font.id,
            Face {
                _source: source.clone(),
            },
        );
    aegle_glyph::register_system_face(font, source);
    issued()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, font);
    Some(font)
}

/// The file path and face index that identify `face`.
fn face_identity(face: &IDWriteFontFace) -> Option<(String, u32)> {
    // SAFETY: the face is live for the call; a null buffer asks for the count.
    let mut count = 0u32;
    unsafe { face.GetFiles(&mut count, None) }.ok()?;
    if count == 0 {
        return None;
    }
    let mut files: Vec<Option<windows::Win32::Graphics::DirectWrite::IDWriteFontFile>> =
        vec![None; count as usize];
    // SAFETY: the vector holds exactly `count` slots, as the first call reported.
    unsafe { face.GetFiles(&mut count, Some(files.as_mut_ptr())) }.ok()?;
    let file = files.into_iter().next().flatten()?;
    // SAFETY: the face is live; `GetIndex` only reads its collection index.
    let index = unsafe { face.GetIndex() };
    Some((file_path(&file)?, index))
}

/// The filesystem path of a DirectWrite font file, when it has one.
fn file_path(file: &windows::Win32::Graphics::DirectWrite::IDWriteFontFile) -> Option<String> {
    // SAFETY: the file is live; the reference key belongs to the loader and is
    // only read through that same loader, and the path buffer is sized to the
    // documented maximum path length.
    unsafe {
        let loader = file.GetLoader().ok()?;
        let local: windows::Win32::Graphics::DirectWrite::IDWriteLocalFontFileLoader =
            loader.cast().ok()?;
        let mut key: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut size = 0u32;
        file.GetReferenceKey(&mut key, &mut size).ok()?;
        if key.is_null() || size == 0 {
            return None;
        }
        let mut units = vec![0u16; 260];
        local.GetFilePathFromKey(key, size, &mut units).ok()?;
        let end = units
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }
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

/// Glyph runs collected during one `Draw` call.
#[derive(Default)]
struct Collected(Vec<StoredRun>);

/// The text renderer DirectWrite calls back into; it only collects glyph runs.
#[implement(IDWriteTextRenderer)]
struct Sink {
    runs: Arc<Mutex<Collected>>,
}

impl IDWritePixelSnapping_Impl for Sink_Impl {
    fn IsPixelSnappingDisabled(&self, _: *const core::ffi::c_void) -> Result<BOOL> {
        // Aegle snaps device-pixel positions itself, in `RasterTransform`.
        Ok(BOOL(1))
    }

    fn GetCurrentTransform(
        &self,
        _: *const core::ffi::c_void,
        transform: *mut DWRITE_MATRIX,
    ) -> Result<()> {
        // SAFETY: DirectWrite passes a writable out pointer for this call.
        unsafe {
            *transform = DWRITE_MATRIX {
                m11: 1.0,
                m12: 0.0,
                m21: 0.0,
                m22: 1.0,
                dx: 0.0,
                dy: 0.0,
            };
        }
        Ok(())
    }

    fn GetPixelsPerDip(&self, _: *const core::ffi::c_void) -> Result<f32> {
        Ok(1.0)
    }
}

impl IDWriteTextRenderer_Impl for Sink_Impl {
    fn DrawGlyphRun(
        &self,
        _: *const core::ffi::c_void,
        baseline_x: f32,
        baseline_y: f32,
        _: DWRITE_MEASURING_MODE,
        glyph_run: *const DWRITE_GLYPH_RUN,
        _: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        // SAFETY: DirectWrite passes a valid glyph run for the duration of this
        // callback, and the font face is an owned COM reference.
        unsafe {
            let run = &*glyph_run;
            let Some(face) = run.fontFace.as_ref() else {
                return Ok(());
            };
            let Some(font) = handle(face) else {
                return Ok(());
            };
            let count = run.glyphCount as usize;
            if count == 0 || run.glyphIndices.is_null() {
                return Ok(());
            }
            let indices = std::slice::from_raw_parts(run.glyphIndices, count);
            let advances = match run.glyphAdvances.is_null() {
                true => None,
                false => Some(std::slice::from_raw_parts(run.glyphAdvances, count)),
            };
            let offsets = match run.glyphOffsets.is_null() {
                true => None,
                false => Some(std::slice::from_raw_parts(run.glyphOffsets, count)),
            };
            // Runs arrive in visual order, so advancing x by each glyph's
            // advance places every glyph correctly even when the run is RTL.
            let mut x = baseline_x;
            let mut glyphs = Vec::with_capacity(count);
            for index in 0..count {
                let advance = advances.map_or(0.0, |advances| advances[index]);
                let offset_x = offsets.map_or(0.0, |offsets| offsets[index].advanceOffset);
                let offset_y = offsets.map_or(0.0, |offsets| offsets[index].ascenderOffset);
                glyphs.push((indices[index], x + offset_x, baseline_y - offset_y));
                x += advance;
            }
            self.runs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .0
                .push(StoredRun {
                    font,
                    size: run.fontEmSize,
                    glyphs,
                });
        }
        Ok(())
    }

    fn DrawUnderline(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: *const windows::Win32::Graphics::DirectWrite::DWRITE_UNDERLINE,
        _: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        // Underlines are recorded by the editor paint path, not by text runs.
        Ok(())
    }

    fn DrawStrikethrough(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: *const windows::Win32::Graphics::DirectWrite::DWRITE_STRIKETHROUGH,
        _: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawInlineObject(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: Ref<IDWriteInlineObject>,
        _: BOOL,
        _: BOOL,
        _: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
}

impl DirectWriteLayout {
    /// Captures the layout's glyph runs; called whenever the layout is rebuilt.
    pub(super) fn capture_runs(&mut self) {
        self.runs.clear();
        let Some(layout) = self.raw() else {
            return;
        };
        let collected = Arc::new(Mutex::new(Collected::default()));
        let sink = Sink {
            runs: Arc::clone(&collected),
        };
        let renderer: IDWriteTextRenderer = sink.into();
        // SAFETY: the renderer outlives the call and the layout keeps no
        // reference to it afterwards; `Draw` only reads layout state.
        if unsafe { layout.Draw(None, &renderer, 0.0, 0.0) }.is_err() {
            return;
        }
        let runs = std::mem::take(&mut collected.lock().unwrap_or_else(PoisonError::into_inner).0);
        self.runs = runs;
    }
}
