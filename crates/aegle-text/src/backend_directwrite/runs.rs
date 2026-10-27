//! Glyph-run capture: DirectWrite faces as backend-independent font handles.
//!
//! DirectWrite's `Draw` calls back once per glyph run with an
//! `IDWriteFontFace`. The scene must not hold that COM pointer, so each face
//! becomes a [`SystemFont`] handle: the process keeps the face in a registry
//! keyed by a monotonically increasing id and tells `aegle-glyph` where its
//! outline data lives ([`aegle_glyph::register_system_face`]). GDI then
//! rasterizes from that file path; no pointer outlives the call.
//!
//! Runs are captured into a sink during `Draw` and copied into the layout's
//! retained run list, so painting and diagnostics need no COM object at all.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use aegle_glyph::FaceSource;
use aegle_glyph::SystemFont;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_GLYPH_RUN, DWRITE_GLYPH_RUN_DESCRIPTION, DWRITE_MATRIX, DWRITE_MEASURING_MODE,
    IDWriteFontFace, IDWriteFontFile, IDWriteInlineObject, IDWritePixelSnapping_Impl,
    IDWriteTextRenderer, IDWriteTextRenderer_Impl,
};
use windows::core::BOOL;
use windows::core::{Interface, Ref, Result, implement};

use super::layout::DirectWriteLayout;

/// Registered DirectWrite faces, by handle id.
fn faces() -> &'static Mutex<BTreeMap<u64, Face>> {
    static FACES: OnceLock<Mutex<BTreeMap<u64, Face>>> = OnceLock::new();
    FACES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Handles already issued for a face object, by its pointer value.
fn issued() -> &'static Mutex<BTreeMap<usize, SystemFont>> {
    static ISSUED: OnceLock<Mutex<BTreeMap<usize, SystemFont>>> = OnceLock::new();
    ISSUED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// One registered face: where its outline data lives.
struct Face {
    /// Readable by the GDI rasterizer.
    _source: FaceSource,
    /// Kept alive so the path stays meaningful.
    _file: Option<IDWriteFontFile>,
}

/// The stable handle for `face`, registering it on first sight.
///
/// The same face object always yields the same handle, so repainting a
/// paragraph reuses one registration. A face that DirectWrite cannot name by
/// file (an in-memory font) is skipped: painting it would have no outline
/// source, and silently drawing nothing is worse than omitting the run.
fn handle(face: &IDWriteFontFace) -> Option<SystemFont> {
    let pointer = face.as_raw() as usize;
    if let Some(font) = issued()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&pointer)
    {
        return Some(*font);
    }
    let source = face_source(face)?;
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
                _file: None,
            },
        );
    aegle_glyph::register_system_face(font, source);
    issued()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(pointer, font);
    Some(font)
}

/// Where GDI can read the bytes of `face`.
fn face_source(face: &IDWriteFontFace) -> Option<FaceSource> {
    // SAFETY: the face is live for the call; a null buffer asks for the count.
    let mut count = 0u32;
    unsafe { face.GetFiles(&mut count, None) }.ok()?;
    if count == 0 {
        return None;
    }
    let mut files: Vec<Option<IDWriteFontFile>> = vec![None; count as usize];
    // SAFETY: the vector holds exactly `count` slots, as the first call reported.
    unsafe { face.GetFiles(&mut count, Some(files.as_mut_ptr())) }.ok()?;
    let file = files.into_iter().next().flatten()?;
    // SAFETY: the face is live; `GetIndex` only reads its collection index.
    let index = unsafe { face.GetIndex() };
    let path = file_path(&file)?;
    Some(FaceSource::File {
        path: Arc::from(path.as_str()),
        index,
    })
}

/// The filesystem path of a DirectWrite font file, when it has one.
fn file_path(file: &IDWriteFontFile) -> Option<String> {
    // SAFETY: the file is live; the reference key is owned by the loader and is
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
