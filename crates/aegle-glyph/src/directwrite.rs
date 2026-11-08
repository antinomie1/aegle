//! DirectWrite glyph rasterizer for Windows: a glyph-run analysis renders each
//! glyph into a grayscale alpha texture.
//!
//! The system antialiases the glyph itself, as FreeType does on Linux, so the
//! output is the same 8-bit coverage mask with baseline-relative placement.
//! `IDWriteFactory2::CreateGlyphRunAnalysis` with grayscale antialiasing yields
//! one byte per pixel through the `DWRITE_TEXTURE_ALIASED_1x1` texture.
//! `RasterOptions::hint` selects grid fitting; synthetic bold is DirectWrite's
//! bold simulation and synthetic slant is the run's transform.
//!
//! A COLR glyph (the system emoji font's representation) is translated with
//! `IDWriteFactory2::TranslateColorGlyphRun` into its layers; each layer's
//! alpha texture is composited in its palette color, or in
//! [`RasterOptions::foreground`] for the text-color entry, into one straight
//! [`Content::Color`] image. A glyph whose only color representation is SVG or
//! a PNG, JPEG, TIFF or BGRA strike returns [`GlyphError::UnsupportedGlyph`]
//! instead of a monochrome silhouette.
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
//! and for `x86_64-pc-windows-msvc` from macOS, and never run on Windows; the
//! COLR layer path likewise.
#![allow(unsafe_code)]

use std::mem::ManuallyDrop;

use aegle_scene::SystemFont;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_COLOR_GLYPH_RUN, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FACE_TYPE,
    DWRITE_FONT_FILE_TYPE, DWRITE_FONT_SIMULATIONS_BOLD, DWRITE_FONT_SIMULATIONS_NONE,
    DWRITE_GLYPH_IMAGE_FORMATS, DWRITE_GLYPH_IMAGE_FORMATS_CFF, DWRITE_GLYPH_IMAGE_FORMATS_COLR,
    DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE, DWRITE_GLYPH_RUN, DWRITE_GRID_FIT_MODE_DISABLED,
    DWRITE_GRID_FIT_MODE_ENABLED, DWRITE_MATRIX, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    DWRITE_TEXTURE_ALIASED_1x1, DWriteCreateFactory, IDWriteFactory, IDWriteFactory2,
    IDWriteFactory5, IDWriteFontFace, IDWriteFontFace4, IDWriteFontFile, IDWriteGlyphRunAnalysis,
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
        let formats = image_formats(&face, glyph)?;
        let outline = DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF;
        let color = (formats & DWRITE_GLYPH_IMAGE_FORMATS_COLR).0 != 0;
        if !color && (formats & !outline).0 != 0 {
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
        let pen = Pen {
            factory: &factory,
            transform: transform.as_ref(),
            options,
        };
        let image = match color {
            true => colr(pen, &run, reserve),
            false => mask(pen, &run, options.offset, reserve),
        };
        // The run borrowed the face; release the reference it was given.
        drop(ManuallyDrop::into_inner(run.fontFace));
        image
    }
}

/// What every layer of one glyph is drawn with.
#[derive(Clone, Copy)]
struct Pen<'a> {
    factory: &'a IDWriteFactory5,
    transform: Option<&'a DWRITE_MATRIX>,
    options: RasterOptions<'a>,
}

impl Pen<'_> {
    /// The grayscale analysis of `run` with its baseline at `origin`, and the
    /// pixel bounds of its alpha texture, y down.
    fn analysis(
        self,
        run: &DWRITE_GLYPH_RUN,
        origin: [f32; 2],
    ) -> Result<(IDWriteGlyphRunAnalysis, RECT), GlyphError> {
        let grid = match self.options.hint {
            true => DWRITE_GRID_FIT_MODE_ENABLED,
            false => DWRITE_GRID_FIT_MODE_DISABLED,
        };
        let factory2: &IDWriteFactory2 = self.factory;
        // SAFETY: the run's face, glyph indices and advances are live for the
        // call, and the transform pointer, when present, points at a value the
        // pen borrows.
        let analysis = unsafe {
            factory2.CreateGlyphRunAnalysis(
                run,
                self.transform.map(|matrix| matrix as *const DWRITE_MATRIX),
                DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
                DWRITE_MEASURING_MODE_NATURAL,
                grid,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                origin[0],
                origin[1],
            )
        }
        .map_err(|_| GlyphError::UnsupportedGlyph)?;
        // SAFETY: the analysis is live; bounds are in pixels relative to the
        // integral origin, y down.
        let bounds = unsafe { analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1) }
            .map_err(|_| GlyphError::UnsupportedGlyph)?;
        Ok((analysis, bounds))
    }
}

/// Pixel width and height of `bounds`, zero when empty.
fn extent(bounds: &RECT) -> (usize, usize) {
    (
        (bounds.right - bounds.left).max(0) as usize,
        (bounds.bottom - bounds.top).max(0) as usize,
    )
}

/// One byte of coverage per pixel of `bounds`.
fn alpha(analysis: &IDWriteGlyphRunAnalysis, bounds: &RECT) -> Result<Vec<u8>, GlyphError> {
    let (width, height) = extent(bounds);
    let mut data = vec![0u8; width * height];
    // SAFETY: `data` holds exactly one byte per pixel of `bounds`, the size
    // an aliased 1x1 texture of those bounds requires.
    unsafe { analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, bounds, &mut data) }
        .map_err(|_| GlyphError::UnsupportedGlyph)?;
    Ok(data)
}

fn placement(bounds: &RECT) -> Placement {
    let (width, height) = extent(bounds);
    Placement {
        left: bounds.left,
        top: -bounds.top,
        width: width as u32,
        height: height as u32,
    }
}

fn empty(reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>) -> Result<Image, GlyphError> {
    reserve(0)?;
    Ok(Image {
        placement: Placement::default(),
        content: Content::Mask,
        data: Vec::new().into_boxed_slice(),
    })
}

/// The outline's grayscale coverage.
fn mask(
    pen: Pen<'_>,
    run: &DWRITE_GLYPH_RUN,
    origin: [f32; 2],
    reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
) -> Result<Image, GlyphError> {
    let (analysis, bounds) = pen.analysis(run, origin)?;
    let (width, height) = extent(&bounds);
    if width == 0 || height == 0 {
        return empty(reserve);
    }
    reserve(width.checked_mul(height).ok_or(GlyphError::ImageBudget)?)?;
    Ok(Image {
        placement: placement(&bounds),
        content: Content::Mask,
        data: alpha(&analysis, &bounds)?.into_boxed_slice(),
    })
}

/// The palette index of a COLR layer drawn in the text color.
const FOREGROUND_LAYER: u16 = 0xFFFF;

/// The COLR layers of the glyph, bottom first, composited source-over in
/// encoded sRGB like the other COLRv0 paths.
fn colr(
    pen: Pen<'_>,
    run: &DWRITE_GLYPH_RUN,
    reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
) -> Result<Image, GlyphError> {
    let factory2: &IDWriteFactory2 = pen.factory;
    let origin = pen.options.offset;
    // SAFETY: the run is live for the call and the transform pointer, when
    // present, points at a value the pen borrows. Palette 0 is the default.
    let layers = unsafe {
        factory2.TranslateColorGlyphRun(
            origin[0],
            origin[1],
            run,
            None,
            DWRITE_MEASURING_MODE_NATURAL,
            pen.transform.map(|matrix| matrix as *const DWRITE_MATRIX),
            0,
        )
    }
    .map_err(|_| GlyphError::UnsupportedGlyph)?;
    // Each layer's run is valid only until the next `MoveNext`, so its
    // analysis is made at once; the analyses own what they need.
    let mut drawn = Vec::new();
    // SAFETY: the enumerator is live.
    while unsafe { layers.MoveNext() }
        .map_err(|_| GlyphError::UnsupportedGlyph)?
        .as_bool()
    {
        // SAFETY: after a `MoveNext` that returned true the current run is
        // valid until the following `MoveNext`, and this borrow ends before it.
        let layer: &DWRITE_COLOR_GLYPH_RUN = unsafe {
            &*layers
                .GetCurrentRun()
                .map_err(|_| GlyphError::UnsupportedGlyph)?
        };
        let ink = match layer.paletteIndex {
            FOREGROUND_LAYER => pen
                .options
                .foreground
                .map(|channel| f32::from(channel) / 255.0),
            _ => {
                let color = layer.runColor;
                [color.r, color.g, color.b, color.a]
            }
        };
        let (analysis, bounds) = pen.analysis(
            &layer.glyphRun,
            [layer.baselineOriginX, layer.baselineOriginY],
        )?;
        if extent(&bounds).0 > 0 && extent(&bounds).1 > 0 {
            drawn.push((analysis, bounds, ink));
        }
    }
    let Some(union) = drawn
        .iter()
        .map(|(_, bounds, _)| *bounds)
        .reduce(|a, b| RECT {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        })
    else {
        return empty(reserve);
    };
    let (width, height) = extent(&union);
    let bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(GlyphError::ImageBudget)?;
    reserve(bytes)?;
    // Premultiplied accumulation in encoded sRGB, one f32 per channel.
    let mut accumulated = vec![[0f32; 4]; width * height];
    for (analysis, bounds, ink) in &drawn {
        let coverage = alpha(analysis, bounds)?;
        let (layer_width, _) = extent(bounds);
        let (dx, dy) = (
            (bounds.left - union.left) as usize,
            (bounds.top - union.top) as usize,
        );
        for (i, &cover) in coverage.iter().enumerate() {
            if cover == 0 {
                continue;
            }
            let a = ink[3].clamp(0.0, 1.0) * f32::from(cover) / 255.0;
            let pixel = &mut accumulated[(dy + i / layer_width) * width + dx + i % layer_width];
            for channel in 0..3 {
                pixel[channel] = ink[channel].clamp(0.0, 1.0) * a + pixel[channel] * (1.0 - a);
            }
            pixel[3] = a + pixel[3] * (1.0 - a);
        }
    }
    let mut data = Vec::with_capacity(bytes);
    for [r, g, b, a] in accumulated {
        let straight = |channel: f32| match a {
            0.0 => 0,
            _ => ((channel / a).clamp(0.0, 1.0) * 255.0).round() as u8,
        };
        data.extend([
            straight(r),
            straight(g),
            straight(b),
            (a * 255.0).round() as u8,
        ]);
    }
    Ok(Image {
        placement: placement(&union),
        content: Content::Color,
        data: data.into_boxed_slice(),
    })
}

/// Every representation the font gives `glyph`, outline included.
fn image_formats(
    face: &IDWriteFontFace,
    glyph: u16,
) -> Result<DWRITE_GLYPH_IMAGE_FORMATS, GlyphError> {
    let face = face
        .cast::<IDWriteFontFace4>()
        .map_err(|_| GlyphError::UnsupportedGlyph)?;
    // SAFETY: the face is live; the full ppem range covers every bitmap strike.
    unsafe { face.GetGlyphImageFormats(glyph, 0, u32::MAX) }.map_err(|_| GlyphError::InvalidFont)
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
