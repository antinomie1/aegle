//! DirectWrite glyph rasterizer for Windows: a glyph-run analysis renders each
//! glyph into a grayscale alpha texture.
//!
//! The system antialiases the glyph itself, as FreeType does on Linux, so the
//! output is the same 8-bit coverage mask with baseline-relative placement.
//! `IDWriteFactory2::CreateGlyphRunAnalysis` with grayscale antialiasing yields
//! one byte per pixel through the `DWRITE_TEXTURE_ALIASED_1x1` texture.
//! `RasterOptions::hint` selects grid fitting; synthetic bold is DirectWrite's
//! bold simulation and synthetic slant is the run's transform. An alpha texture
//! carries no color, so a glyph whose font gives it a color or bitmap
//! representation (COLR, SVG, PNG, JPEG, TIFF or BGRA strikes) returns
//! [`GlyphError::UnsupportedGlyph`] instead of a monochrome silhouette.
//!
//! Font faces come from the face's file path or, for registered bytes, from an
//! in-memory font file loader registered with the shared factory on first use
//! and unregistered when the rasterizer drops. At most [`MAX_FACES`] faces stay
//! alive, evicting the least recently used. Font variation coordinates are not
//! applied, matching the earlier outline path.
//!
//! # Verification
//!
//! Compile-checked only: cross-compiled for `x86_64-pc-windows-gnu` from Linux
//! and never run on Windows.
#![allow(unsafe_code)]

use std::mem::ManuallyDrop;

use aegle_scene::SystemFont;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FACE_TYPE, DWRITE_FONT_FILE_TYPE,
    DWRITE_FONT_SIMULATIONS_BOLD, DWRITE_FONT_SIMULATIONS_NONE, DWRITE_GLYPH_IMAGE_FORMATS_CFF,
    DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE, DWRITE_GLYPH_RUN, DWRITE_GRID_FIT_MODE_DISABLED,
    DWRITE_GRID_FIT_MODE_ENABLED, DWRITE_MATRIX, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    DWRITE_TEXTURE_ALIASED_1x1, DWriteCreateFactory, IDWriteFactory, IDWriteFactory2,
    IDWriteFactory5, IDWriteFontFace, IDWriteFontFace4, IDWriteFontFile,
    IDWriteInMemoryFontFileLoader,
};
use windows::core::{BOOL, IUnknown, Interface, PCWSTR};

use crate::system::{FaceSource, SystemRasterizer, register_system_rasterizer};
use crate::{CacheLimits, Content, GlyphError, Image, Placement, RasterOptions};

/// Faces kept alive at once; four matches the FreeType face limit.
const MAX_FACES: usize = 4;

/// One cached face and what it was built from.
struct Face {
    source: FaceSource,
    embolden: bool,
    face: IDWriteFontFace,
}

/// DirectWrite-backed rasterizer for platform font handles.
pub struct DirectWriteRasterizer {
    faces: Vec<Face>,
    /// Loader for byte-backed faces, registered with `factory` once created.
    memory: Option<IDWriteInMemoryFontFileLoader>,
    /// The shared factory, created on the first glyph.
    factory: Option<IDWriteFactory5>,
}

impl DirectWriteRasterizer {
    /// Creates an empty rasterizer; nothing is allocated until a glyph is drawn.
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            memory: None,
            factory: None,
        }
    }

    fn factory(&mut self) -> Result<IDWriteFactory5, GlyphError> {
        if let Some(factory) = &self.factory {
            return Ok(factory.clone());
        }
        // SAFETY: the shared factory type may be created from any thread, and
        // the returned interface is reference counted by its wrapper.
        let factory: IDWriteFactory5 = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|_| GlyphError::UnsupportedGlyph)?;
        self.factory = Some(factory.clone());
        Ok(factory)
    }

    /// A face for `source`, reusing or evicting a cache entry.
    fn face(&mut self, source: &FaceSource, embolden: bool) -> Result<IDWriteFontFace, GlyphError> {
        if let Some(index) = self
            .faces
            .iter()
            .position(|face| &face.source == source && face.embolden == embolden)
        {
            let face = self.faces.remove(index);
            let handle = face.face.clone();
            self.faces.push(face);
            return Ok(handle);
        }
        let factory = self.factory()?;
        let (file, index) = match source {
            FaceSource::File { path, index } => {
                let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
                // SAFETY: `wide` is NUL-terminated and outlives the call, which
                // copies the path into the returned file reference.
                let file = unsafe { factory.CreateFontFileReference(PCWSTR(wide.as_ptr()), None) }
                    .map_err(|_| GlyphError::InvalidFont)?;
                (file, *index)
            }
            FaceSource::Bytes { data, index } => (self.memory_file(&factory, data)?, *index),
        };
        let face = create_face(&factory, file, index, embolden)?;
        if self.faces.len() >= MAX_FACES {
            self.faces.remove(0);
        }
        self.faces.push(Face {
            source: source.clone(),
            embolden,
            face: face.clone(),
        });
        Ok(face)
    }

    /// A font file reference to a copy of `data` held by the in-memory loader.
    fn memory_file(
        &mut self,
        factory: &IDWriteFactory5,
        data: &[u8],
    ) -> Result<IDWriteFontFile, GlyphError> {
        let loader = match &self.memory {
            Some(loader) => loader.clone(),
            None => {
                // SAFETY: the factory is live; the loader is reference counted
                // and registered once, then unregistered in `Drop`.
                let loader = unsafe { factory.CreateInMemoryFontFileLoader() }
                    .map_err(|_| GlyphError::UnsupportedGlyph)?;
                unsafe { factory.RegisterFontFileLoader(&loader) }
                    .map_err(|_| GlyphError::UnsupportedGlyph)?;
                self.memory = Some(loader.clone());
                loader
            }
        };
        let length = u32::try_from(data.len()).map_err(|_| GlyphError::InvalidFont)?;
        // SAFETY: `data` is live for the call; without an owner object the
        // loader copies the bytes, so the reference never points into `data`.
        unsafe {
            loader.CreateInMemoryFontFileReference(
                factory,
                data.as_ptr().cast(),
                length,
                None::<&IUnknown>,
            )
        }
        .map_err(|_| GlyphError::InvalidFont)
    }
}

impl Drop for DirectWriteRasterizer {
    fn drop(&mut self) {
        // Faces may reference the loader's files, so they go first.
        self.faces.clear();
        if let (Some(factory), Some(loader)) = (&self.factory, &self.memory) {
            // SAFETY: the loader was registered with this factory in
            // `memory_file` and is unregistered exactly once, here.
            let _ = unsafe { factory.UnregisterFontFileLoader(loader) };
        }
    }
}

// SAFETY: DirectWrite objects from the shared factory are free-threaded. The
// rasterizer is moved to a thread only as a whole and never shared (the
// registry holds one and calls it under a lock).
unsafe impl Send for DirectWriteRasterizer {}

impl Default for DirectWriteRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemRasterizer for DirectWriteRasterizer {
    fn rasterize(
        &mut self,
        font: &SystemFont,
        glyph: u16,
        options: RasterOptions<'_>,
        _limits: CacheLimits,
        reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        let source = crate::system_face(*font).ok_or(GlyphError::InvalidFont)?;
        let face = self.face(&source, options.embolden)?;
        if has_color(&face, glyph)? {
            return Err(GlyphError::UnsupportedGlyph);
        }
        let factory = self.factory()?;
        let advance = 0.0f32;
        let run = DWRITE_GLYPH_RUN {
            fontFace: ManuallyDrop::new(Some(face)),
            fontEmSize: options.size,
            glyphCount: 1,
            glyphIndices: &glyph,
            glyphAdvances: &advance,
            glyphOffsets: std::ptr::null(),
            isSideways: BOOL(0),
            bidiLevel: 0,
        };
        // DirectWrite is y-down like the offset, so a positive slant moves
        // points above the baseline (negative y) to the right.
        let transform = (options.skew != 0).then(|| DWRITE_MATRIX {
            m11: 1.0,
            m12: 0.0,
            m21: -(f64::from(options.skew).to_radians().tan() as f32),
            m22: 1.0,
            dx: 0.0,
            dy: 0.0,
        });
        let grid = match options.hint {
            true => DWRITE_GRID_FIT_MODE_ENABLED,
            false => DWRITE_GRID_FIT_MODE_DISABLED,
        };
        let factory2: &IDWriteFactory2 = &factory;
        // SAFETY: the run's face, glyph index and advance are live for the
        // call, and the transform pointer, when present, points at a local.
        let analysis = unsafe {
            factory2.CreateGlyphRunAnalysis(
                &run,
                transform
                    .as_ref()
                    .map(|matrix| matrix as *const DWRITE_MATRIX),
                DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
                DWRITE_MEASURING_MODE_NATURAL,
                grid,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                options.offset[0],
                options.offset[1],
            )
        };
        // The run borrowed the face; release the reference it was given.
        drop(ManuallyDrop::into_inner(run.fontFace));
        let analysis = analysis.map_err(|_| GlyphError::UnsupportedGlyph)?;
        // SAFETY: the analysis is live; bounds are in pixels relative to the
        // integral origin, y down.
        let bounds = unsafe { analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1) }
            .map_err(|_| GlyphError::UnsupportedGlyph)?;
        let width = (bounds.right - bounds.left).max(0) as usize;
        let height = (bounds.bottom - bounds.top).max(0) as usize;
        if width == 0 || height == 0 {
            reserve(0)?;
            return Ok(Image {
                placement: Placement::default(),
                content: Content::Mask,
                data: Vec::new().into_boxed_slice(),
            });
        }
        let bytes = width.checked_mul(height).ok_or(GlyphError::ImageBudget)?;
        reserve(bytes)?;
        let mut data = vec![0u8; bytes];
        // SAFETY: `data` holds exactly one byte per pixel of `bounds`, the size
        // an aliased 1x1 texture of those bounds requires.
        unsafe { analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &bounds, &mut data) }
            .map_err(|_| GlyphError::UnsupportedGlyph)?;
        Ok(Image {
            placement: Placement {
                left: bounds.left,
                top: -bounds.top,
                width: width as u32,
                height: height as u32,
            },
            content: Content::Mask,
            data: data.into_boxed_slice(),
        })
    }
}

/// Whether the font gives `glyph` any representation besides its outline.
fn has_color(face: &IDWriteFontFace, glyph: u16) -> Result<bool, GlyphError> {
    let face = face
        .cast::<IDWriteFontFace4>()
        .map_err(|_| GlyphError::UnsupportedGlyph)?;
    // SAFETY: the face is live; the full ppem range covers every bitmap strike.
    let formats = unsafe { face.GetGlyphImageFormats(glyph, 0, u32::MAX) }
        .map_err(|_| GlyphError::InvalidFont)?;
    let outline = DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF;
    Ok((formats & !outline).0 != 0)
}

/// Creates the face at `index` of `file`, with bold simulation when asked.
fn create_face(
    factory: &IDWriteFactory,
    file: IDWriteFontFile,
    index: u32,
    embolden: bool,
) -> Result<IDWriteFontFace, GlyphError> {
    let mut supported = BOOL(0);
    let mut file_type = DWRITE_FONT_FILE_TYPE(0);
    let mut face_type = DWRITE_FONT_FACE_TYPE(0);
    let mut faces = 0u32;
    // SAFETY: the file is live and every out pointer is a live local.
    unsafe {
        file.Analyze(
            &mut supported,
            &mut file_type,
            Some(&mut face_type),
            &mut faces,
        )
    }
    .map_err(|_| GlyphError::InvalidFont)?;
    if !supported.as_bool() || index >= faces {
        return Err(GlyphError::InvalidFont);
    }
    let simulations = match embolden {
        true => DWRITE_FONT_SIMULATIONS_BOLD,
        false => DWRITE_FONT_SIMULATIONS_NONE,
    };
    // SAFETY: the factory and file are live; the face takes its own reference
    // to the file.
    unsafe { factory.CreateFontFace(face_type, &[Some(file)], index, simulations) }
        .map_err(|_| GlyphError::InvalidFont)
}

/// Registers the DirectWrite rasterizer for [`SystemFont::DWRITE`].
///
/// Called once by `aegle-text`'s DirectWrite backend; returns false if another
/// rasterizer already claimed that backend number.
pub fn register() -> bool {
    register_system_rasterizer(SystemFont::DWRITE, Box::new(DirectWriteRasterizer::new()))
}
