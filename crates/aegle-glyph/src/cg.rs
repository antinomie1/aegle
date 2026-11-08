//! CoreGraphics glyph rasterizer for macOS: CoreText draws each glyph into an
//! alpha-only bitmap context.
//!
//! The system antialiases the glyph itself, as FreeType does on Linux, so the
//! output is the same 8-bit coverage mask with baseline-relative placement.
//! Font smoothing (CoreGraphics' stem darkening for LCD output) is off, so the
//! coverage is the plain antialiased area the renderers' contrast curve expects.
//! Synthetic slant is the text matrix, and synthetic bold strokes the glyph's
//! outline while filling it, growing every side by half of `size / 32` like
//! `FT_Outline_Embolden`. An alpha-only context drops color, and CoreText
//! reports color glyphs (sbix, COLR, SVG) only per font, so every visible glyph
//! of a font with the color-glyphs trait returns
//! [`GlyphError::UnsupportedGlyph`] instead of a monochrome silhouette.
//!
//! CoreText's own runs name the `CTFont` they were shaped with through
//! [`register_font`], so the glyph comes from exactly that face, variable
//! instance and application font included; the rasterizer only resizes it.
//! Shared font bytes ([`FaceSource`]) open the face at their collection index.
//! Every font object is reference counted by `CFRetained`, and at most
//! [`MAX_FONTS`] sized fonts stay alive, evicting the least recently used.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock, PoisonError};

use aegle_scene::SystemFont;
use objc2_core_foundation::{
    CFArray, CFData, CFRetained, CFURL, CGAffineTransform, CGPoint, CGRect,
};
use objc2_core_graphics::{CGBitmapContextCreate, CGContext, CGImageAlphaInfo, CGTextDrawingMode};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromData,
    CTFontManagerCreateFontDescriptorsFromURL, CTFontOrientation, CTFontSymbolicTraits,
};

use crate::system::{FaceSource, SystemRasterizer, register_system_rasterizer};
use crate::{CacheLimits, Content, GlyphError, Image, Placement, RasterOptions};

/// Fonts kept alive at once; four matches the FreeType and DirectWrite limits.
const MAX_FONTS: usize = 4;
/// Largest glyph bound magnitude in device pixels, matching the renderers'
/// device range.
const MAX_COORD: f64 = 1_048_576.0;

/// A registered CoreText font.
struct Native(CFRetained<CTFont>);

// SAFETY: CoreText documents font objects as immutable and safe to use from
// any thread; the registry only hands out new references under its lock.
unsafe impl Send for Native {}

/// Fonts CoreText shaped with, by the handle their runs carry.
fn natives() -> &'static Mutex<BTreeMap<SystemFont, Native>> {
    static NATIVES: OnceLock<Mutex<BTreeMap<SystemFont, Native>>> = OnceLock::new();
    NATIVES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Makes `handle` rasterize with `font`'s face at any size, replacing any
/// earlier registration. The CoreText text backend calls this before a run
/// naming `handle` is painted.
pub fn register_font(handle: SystemFont, font: &CTFont) {
    natives()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, Native(objc2_core_foundation::Type::retain(font)));
}

/// One cached font, by the handle and size it was created for.
struct Cached {
    handle: SystemFont,
    font: CFRetained<CTFont>,
    /// Bits of the `f32` pixels per em the font was created at.
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

    /// The font of `handle` at `size` pixels per em, reusing or evicting a
    /// cache entry.
    fn font(&mut self, handle: SystemFont, size: f32) -> Result<&CFRetained<CTFont>, GlyphError> {
        let bits = size.to_bits();
        if let Some(index) = self
            .fonts
            .iter()
            .position(|cached| cached.handle == handle && cached.size == bits)
        {
            let cached = self.fonts.remove(index);
            self.fonts.push(cached);
            return Ok(&self.fonts.last().expect("just pushed").font);
        }
        let font = build_font(handle, size)?;
        if self.fonts.len() >= MAX_FONTS {
            self.fonts.remove(0);
        }
        self.fonts.push(Cached {
            handle,
            font,
            size: bits,
        });
        Ok(&self.fonts.last().expect("just pushed").font)
    }
}

// SAFETY: CoreText font objects are immutable and CoreText documents them as
// safe to use from any thread; the rasterizer is only ever moved as a whole,
// never shared between threads concurrently (the registry holds one and calls
// it under a lock). Bitmap contexts live only inside one `rasterize` call.
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
        let font = self.font(*font, options.size)?.clone();
        let mut glyph = glyph;
        let mut bounds = CGRect::default();
        // SAFETY: the font is live; one glyph and one output rectangle are
        // passed with a count of one. The return value is the union, which for
        // one glyph equals the rectangle written.
        unsafe {
            font.bounding_rects_for_glyphs(
                CTFontOrientation::Default,
                NonNull::from(&mut glyph),
                &mut bounds,
                1,
            );
        }
        let skew = match options.skew {
            0 => 0.0,
            degrees => f64::from(degrees).to_radians().tan(),
        };
        let embolden = match options.embolden {
            true => f64::from(options.size) / 32.0,
            false => 0.0,
        };
        // Device space here is y-up from the integral baseline; the glyph origin
        // sits at the fractional offset (whose y is positive downward).
        let origin = (f64::from(options.offset[0]), -f64::from(options.offset[1]));
        let (low, high) = (bounds.origin.y, bounds.origin.y + bounds.size.height);
        // The slant moves x by `skew * y`, so its extremes are at the vertical
        // bounds. One pixel of margin keeps antialiased edges inside the image.
        let grow = embolden / 2.0 + 1.0;
        let left = bounds.origin.x + (skew * low).min(skew * high) + origin.0 - grow;
        let right =
            bounds.origin.x + bounds.size.width + (skew * low).max(skew * high) + origin.0 + grow;
        let bottom = low + origin.1 - grow;
        let top = high + origin.1 + grow;
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            reserve(0)?;
            return Ok(empty());
        }
        // SAFETY: the font is live; reading its traits has no other effect.
        let traits = unsafe { font.symbolic_traits() };
        if traits.contains(CTFontSymbolicTraits::TraitColorGlyphs) {
            return Err(GlyphError::UnsupportedGlyph);
        }
        if ![left, right, bottom, top]
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_COORD)
        {
            return Err(GlyphError::Coordinates);
        }
        let (left, right, bottom, top) = (left.floor(), right.ceil(), bottom.floor(), top.ceil());
        let width = (right - left) as usize;
        let height = (top - bottom) as usize;
        let bytes = width.checked_mul(height).ok_or(GlyphError::ImageBudget)?;
        reserve(bytes)?;
        let mut data = vec![0u8; bytes];
        // SAFETY: `data` holds exactly `width * height` bytes, one per pixel with
        // rows `width` bytes apart, and outlives the context, which is dropped
        // at the end of this function. A null color space with alpha-only
        // `bitmap_info` is the documented form for an 8-bit alpha context.
        let context = unsafe {
            CGBitmapContextCreate(
                data.as_mut_ptr().cast(),
                width,
                height,
                8,
                width,
                None,
                CGImageAlphaInfo::Only.0,
            )
        }
        .ok_or(GlyphError::ImageBudget)?;
        draw(
            &context,
            &font,
            glyph,
            skew,
            embolden,
            (origin.0 - left, origin.1 - bottom),
        );
        drop(context);
        // The bitmap's first row is its top edge, so `data` is already top-down.
        Ok(Image {
            placement: Placement {
                left: left as i32,
                top: top as i32,
                width: width as u32,
                height: height as u32,
            },
            content: Content::Mask,
            data: data.into_boxed_slice(),
        })
    }
}

/// Draws `glyph` with its origin at `position` in the context's y-up pixels.
fn draw(
    context: &CGContext,
    font: &CTFont,
    glyph: u16,
    skew: f64,
    embolden: f64,
    position: (f64, f64),
) {
    let context_ref = Some(context);
    CGContext::set_should_antialias(context_ref, true);
    CGContext::set_allows_font_smoothing(context_ref, false);
    CGContext::set_should_smooth_fonts(context_ref, false);
    CGContext::set_allows_font_subpixel_positioning(context_ref, true);
    CGContext::set_should_subpixel_position_fonts(context_ref, true);
    CGContext::set_allows_font_subpixel_quantization(context_ref, false);
    CGContext::set_should_subpixel_quantize_fonts(context_ref, false);
    // In an alpha-only context only the alpha of a color is stored.
    CGContext::set_gray_fill_color(context_ref, 0.0, 1.0);
    CGContext::set_text_matrix(
        context_ref,
        CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: skew,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        },
    );
    if embolden > 0.0 {
        CGContext::set_gray_stroke_color(context_ref, 0.0, 1.0);
        CGContext::set_line_width(context_ref, embolden);
        CGContext::set_text_drawing_mode(context_ref, CGTextDrawingMode::FillStroke);
    }
    let mut glyph = glyph;
    let mut point = CGPoint {
        x: position.0,
        y: position.1,
    };
    // SAFETY: the font and context are live for the call, and exactly one glyph
    // and one position are passed with a count of one.
    unsafe {
        font.draw_glyphs(
            NonNull::from(&mut glyph),
            NonNull::from(&mut point),
            1,
            context,
        );
    }
}

fn empty() -> Image {
    Image {
        placement: Placement::default(),
        content: Content::Mask,
        data: Vec::new().into_boxed_slice(),
    }
}

/// Builds the font of `handle` at `pixels` pixels per em: a registered
/// CoreText font resized, or the face a [`FaceSource`] names.
fn build_font(handle: SystemFont, pixels: f32) -> Result<CFRetained<CTFont>, GlyphError> {
    let size = f64::from(pixels);
    let native = natives()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&handle)
        .map(|native| native.0.clone());
    if let Some(font) = native {
        // SAFETY: the font is live; a null matrix keeps its own and no
        // attributes are added.
        return Ok(unsafe { font.copy_with_attributes(size, std::ptr::null(), None) });
    }
    let source = crate::system_face(handle).ok_or(GlyphError::InvalidFont)?;
    let (faces, index) = match &source {
        FaceSource::File { path, index } => {
            let url = CFURL::from_file_path(&**path).ok_or(GlyphError::InvalidFont)?;
            // SAFETY: the URL is live; the result is reference counted.
            let faces = unsafe { CTFontManagerCreateFontDescriptorsFromURL(&url) };
            (faces.ok_or(GlyphError::InvalidFont)?, *index)
        }
        FaceSource::Bytes { data, index } => {
            // `CFData` copies the bytes, and the descriptors retain it.
            let data = CFData::from_bytes(data);
            // SAFETY: the data is live; the result is reference counted.
            let faces = unsafe { CTFontManagerCreateFontDescriptorsFromData(&data) };
            (faces, *index)
        }
    };
    // SAFETY: CoreText fills these arrays with font descriptors, in the
    // order of the faces in the file, so a collection index selects one.
    let faces: CFRetained<CFArray<CTFontDescriptor>> = unsafe { CFRetained::cast_unchecked(faces) };
    let face = faces.get(index as usize).ok_or(GlyphError::InvalidFont)?;
    // SAFETY: the descriptor is live; a null matrix is the identity.
    Ok(unsafe { CTFont::with_font_descriptor(&face, size, std::ptr::null()) })
}

/// Registers the CoreGraphics rasterizer for [`SystemFont::CORETEXT`].
///
/// Called once by `aegle-text`'s CoreText backend; returns false if another
/// rasterizer already claimed that backend number.
pub fn register() -> bool {
    register_system_rasterizer(SystemFont::CORETEXT, Box::new(CgRasterizer::new()))
}
