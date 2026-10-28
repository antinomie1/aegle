//! CoreGraphics outline rasterizer for macOS, driven by
//! `CTFontCreatePathForGlyph`.
//!
//! `CTFontCreatePathForGlyph` returns the glyph outline as a `CGPath` in font
//! units scaled to the requested size, y up from the baseline. This module
//! walks that path into the shared [`OutlineBuilder`], so macOS and Windows
//! produce coverage from the same rasterizer. CoreGraphics can hand back color
//! glyphs through a separate API this module does not use, so COLR/CBDT/SVG
//! glyphs return [`GlyphError::UnsupportedGlyph`] rather than a monochrome
//! approximation.
//!
//! Every font object is reference counted by `CFRetained`, and at most
//! [`MAX_FONTS`] stay alive, evicting the least recently used.
//!
//! # Verification
//!
//! Compile-checked only: cross-compiled for `aarch64-apple-darwin` from Linux
//! and never run on a Mac.
#![allow(unsafe_code)]

use aegle_scene::SystemFont;
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{CGDataProvider, CGFont, CGPath, CGPathElement, CGPathElementType};
use objc2_core_text::CTFont;

use crate::system::{FaceSource, SystemRasterizer, register_system_rasterizer};
use crate::{CacheLimits, GlyphError, Image, OutlineBuilder, RasterOptions};

/// Fonts kept alive at once; four matches the FreeType and GDI limits.
const MAX_FONTS: usize = 4;

/// One cached font and the face source it was built from.
struct Cached {
    source: FaceSource,
    font: CFRetained<CTFont>,
    /// Pixels per em the font was created at; CoreText paths scale with it.
    size: u32,
}

/// CoreGraphics-backed rasterizer for platform font handles.
pub struct CgRasterizer {
    fonts: Vec<Cached>,
}

impl CgRasterizer {
    /// Creates an empty rasterizer; nothing is allocated until a glyph is drawn.
    pub fn new() -> Self {
        Self { fonts: Vec::new() }
    }

    /// A font for `source` at `size` pixels per em, reusing or evicting a cache
    /// entry.
    fn font(&mut self, source: &FaceSource, size: u32) -> Result<&CFRetained<CTFont>, GlyphError> {
        if let Some(index) = self
            .fonts
            .iter()
            .position(|cached| &cached.source == source && cached.size == size)
        {
            let cached = self.fonts.remove(index);
            self.fonts.push(cached);
            return Ok(&self.fonts.last().expect("just pushed").font);
        }
        let font = build_font(source, size)?;
        if self.fonts.len() >= MAX_FONTS {
            self.fonts.remove(0);
        }
        self.fonts.push(Cached {
            source: source.clone(),
            font,
            size,
        });
        Ok(&self.fonts.last().expect("just pushed").font)
    }
}

// SAFETY: CoreText font objects are immutable and CoreText documents them as
// safe to use from any thread; the rasterizer is only ever moved as a whole,
// never shared between threads concurrently (the registry holds one and calls
// it under a lock).
unsafe impl Send for CgRasterizer {}

impl Default for CgRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemRasterizer for CgRasterizer {
    fn rasterize(
        &mut self,
        font: &SystemFont,
        glyph: u16,
        options: RasterOptions<'_>,
        _limits: CacheLimits,
        reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        let source = crate::system_face(*font).ok_or(GlyphError::InvalidFont)?;
        let pixels = options.size.round().clamp(1.0, 16_384.0) as u32;
        let font = self.font(&source, pixels)?.clone();
        // SAFETY: the font is live and owned; a null matrix means the identity
        // transform. The returned path is reference counted, and `None` means
        // CoreGraphics has no outline for this glyph.
        let path = unsafe { font.path_for_glyph(glyph, std::ptr::null()) };
        let Some(path) = path else {
            return Err(GlyphError::UnsupportedGlyph);
        };
        let mut outline = OutlineBuilder::new(options.size, options.embolden, options.skew);
        // SAFETY: the path is live for the call; the applier only reads it and
        // passes each element to `visit`, which copies the points it needs.
        unsafe {
            CGPath::apply(
                Some(&path),
                &mut outline as *mut OutlineBuilder as *mut core::ffi::c_void,
                Some(visit),
            );
        }
        let offset = [options.offset[0], -options.offset[1]];
        outline.rasterize(offset, reserve)
    }
}

/// Appends one `CGPath` element to the builder in `info`.
///
/// # Safety
///
/// `info` must point at a live `OutlineBuilder`, which is how `rasterize` calls
/// this; CoreGraphics guarantees the element's points array has as many entries
/// as its type requires.
unsafe extern "C-unwind" fn visit(
    info: *mut core::ffi::c_void,
    element: std::ptr::NonNull<CGPathElement>,
) {
    let builder = unsafe { &mut *info.cast::<OutlineBuilder>() };
    let element = unsafe { element.as_ref() };
    let points = element.points.as_ptr();
    // SAFETY: the point count per element type is fixed by CoreGraphics, and
    // the array is valid for the duration of the callback.
    unsafe {
        match element.r#type {
            CGPathElementType::MoveToPoint => {
                let p = *points;
                builder.move_to(p.x as f32, p.y as f32);
            }
            CGPathElementType::AddLineToPoint => {
                let p = *points;
                builder.line_to(p.x as f32, p.y as f32);
            }
            CGPathElementType::AddQuadCurveToPoint => {
                let control = *points;
                let end = *points.add(1);
                builder.quad_to(
                    control.x as f32,
                    control.y as f32,
                    end.x as f32,
                    end.y as f32,
                );
            }
            CGPathElementType::AddCurveToPoint => {
                let c1 = *points;
                let c2 = *points.add(1);
                let end = *points.add(2);
                builder.cubic_to(
                    c1.x as f32,
                    c1.y as f32,
                    c2.x as f32,
                    c2.y as f32,
                    end.x as f32,
                    end.y as f32,
                );
            }
            CGPathElementType::CloseSubpath => builder.close(),
            _ => {}
        }
    }
}

/// Builds a `CTFont` for `source` at `pixels` pixels per em.
///
/// A file-backed face is opened by URL; a byte-backed one is turned into a
/// `CGFont` from a data provider. Both are then sized through
/// `CTFontCreateWithGraphicsFont`, so the path CoreGraphics returns is in the
/// same coordinate space either way.
fn build_font(source: &FaceSource, pixels: u32) -> Result<CFRetained<CTFont>, GlyphError> {
    let graphics = match source {
        FaceSource::File { path, .. } => {
            let name = objc2_core_foundation::CFString::from_str(path);
            CGFont::with_font_name(Some(&name)).ok_or(GlyphError::InvalidFont)?
        }
        FaceSource::Bytes { data, .. } => {
            // `CFData` copies the bytes, so the provider and font own
            // everything they need afterwards.
            let cf_data = CFData::from_bytes(data);
            let provider =
                CGDataProvider::with_cf_data(Some(&cf_data)).ok_or(GlyphError::InvalidFont)?;
            CGFont::with_data_provider(&provider).ok_or(GlyphError::InvalidFont)?
        }
    };
    // SAFETY: the graphics font is live and owned; the returned font is
    // reference counted and takes its own reference to the graphics font.
    let font =
        unsafe { CTFont::with_graphics_font(&graphics, f64::from(pixels), std::ptr::null(), None) };
    Ok(font)
}

/// Registers the CoreGraphics rasterizer for [`SystemFont::CORETEXT`].
///
/// Called once by `aegle-text`'s CoreText backend; returns false if another
/// rasterizer already claimed that backend number.
pub fn register() -> bool {
    register_system_rasterizer(SystemFont::CORETEXT, Box::new(CgRasterizer::new()))
}
