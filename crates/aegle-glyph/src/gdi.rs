//! GDI outline rasterizer for Windows, driven by `GetGlyphOutlineW`.
//!
//! `GGO_NATIVE` returns the font's own quadratic/cubic contours as
//! `TTPOLYGONHEADER`/`TTPOLYCURVE` records in 26.6 fixed point. This module
//! converts them to the shared [`OutlineBuilder`] path, so Windows and macOS
//! produce coverage from the same rasterizer. GDI cannot expose COLR or
//! embedded-bitmap glyphs, so those return [`GlyphError::UnsupportedGlyph`]
//! rather than a monochrome approximation.
//!
//! Every GDI object has one owner and is released in `Drop`: the in-memory DC,
//! each selected `HFONT`, and the `AddFontMemResourceEx` handles. The font file
//! is registered once per source and reused; at most [`MAX_FACES`] device
//! contexts stay alive, evicting the least recently used.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::Arc;

use aegle_scene::SystemFont;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, AddFontMemResourceEx, CLEARTYPE_QUALITY, CreateCompatibleDC,
    CreateFontIndirectW, DEFAULT_CHARSET, DeleteDC, DeleteObject, FIXED, GET_GLYPH_OUTLINE_FORMAT,
    GGO_GLYPH_INDEX, GGO_NATIVE, GGO_UNHINTED, GLYPHMETRICS, GetGlyphOutlineW, HDC, HFONT, HGDIOBJ,
    LOGFONTW, MAT2, POINTFX, RemoveFontMemResourceEx, SelectObject, TT_PRIM_CSPLINE, TT_PRIM_LINE,
    TT_PRIM_QSPLINE, TTPOLYGONHEADER,
};

use crate::system::{FaceSource, SystemRasterizer, register_system_rasterizer};
use crate::{
    CacheLimits, Content, GlyphError, Image, OutlineBuilder, Placement, RasterOptions, describe,
};

/// Device contexts kept alive at once; four matches the FreeType face limit.
const MAX_FACES: usize = 4;
/// 16.16 fixed point: one pixel is this many raw units.
const FIXED_ONE: f32 = 65_536.0;

/// One GDI device context with its currently selected font.
struct Device {
    dc: HDC,
    font: HFONT,
    key: FaceKey,
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: both handles were created by this module and are released
        // exactly once, here. Selecting a null object first lets the font be
        // deleted while it is still selected.
        unsafe {
            if !self.font.is_invalid() {
                SelectObject(self.dc, HGDIOBJ(std::ptr::null_mut()));
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
            if !self.dc.is_invalid() {
                let _ = DeleteDC(self.dc);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FaceKey {
    family: Arc<str>,
    weight: u16,
    italic: bool,
    pixels: u32,
    hint: bool,
}

/// A registered in-memory font; dropping it removes the registration.
struct MemoryFont {
    handle: HANDLE,
}

impl Drop for MemoryFont {
    fn drop(&mut self) {
        // SAFETY: the handle came from `AddFontMemResourceEx` and is released
        // once, after every device context using it has been dropped first
        // (fields drop in declaration order: devices before fonts).
        unsafe {
            if !self.handle.is_invalid() {
                let _ = RemoveFontMemResourceEx(self.handle);
            }
        }
    }
}

/// GDI-backed rasterizer for system fonts whose bytes this process can read.
pub struct GdiRasterizer {
    /// Devices first so they drop before the fonts they use.
    devices: Vec<Device>,
    fonts: BTreeMap<FaceSource, MemoryFont>,
}

impl GdiRasterizer {
    /// Creates an empty rasterizer; nothing is allocated until a glyph is drawn.
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            fonts: BTreeMap::new(),
        }
    }

    fn memory_font(&mut self, source: &FaceSource, bytes: &[u8]) -> Result<(), GlyphError> {
        if self.fonts.contains_key(source) {
            return Ok(());
        }
        if bytes.is_empty() {
            return Err(GlyphError::InvalidFont);
        }
        let count = 0u32;
        // SAFETY: `bytes` is live for the call; GDI copies the data it needs and
        // the returned handle owns the registration until `MemoryFont` drops.
        let handle = unsafe {
            AddFontMemResourceEx(
                bytes.as_ptr() as *const c_void,
                bytes.len() as u32,
                None,
                &count,
            )
        };
        if handle.is_invalid() || count == 0 {
            return Err(GlyphError::InvalidFont);
        }
        self.fonts.insert(source.clone(), MemoryFont { handle });
        Ok(())
    }

    /// Selects a font for `key`, reusing or evicting a device context.
    fn device(&mut self, key: &FaceKey) -> Result<&mut Device, GlyphError> {
        if let Some(index) = self.devices.iter().position(|device| &device.key == key) {
            let device = self.devices.remove(index);
            self.devices.push(device);
            return Ok(self.devices.last_mut().expect("just pushed"));
        }
        let mut logfont = LOGFONTW {
            lfHeight: -(key.pixels as i32),
            lfWeight: i32::from(key.weight),
            lfItalic: u8::from(key.italic),
            lfCharSet: DEFAULT_CHARSET,
            lfQuality: match key.hint {
                true => ANTIALIASED_QUALITY,
                false => CLEARTYPE_QUALITY,
            },
            ..Default::default()
        };
        for (slot, unit) in logfont
            .lfFaceName
            .iter_mut()
            .zip(key.family.encode_utf16().chain([0]))
        {
            *slot = unit;
        }
        // SAFETY: the face name is NUL-terminated inside `LF_FACESIZE` slots;
        // each handle is stored in `Device` and released by its `Drop`.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(GlyphError::InvalidFont);
            }
            let font = CreateFontIndirectW(&logfont);
            if font.is_invalid() {
                let _ = DeleteDC(dc);
                return Err(GlyphError::InvalidFont);
            }
            if SelectObject(dc, HGDIOBJ(font.0)).is_invalid() {
                let _ = DeleteObject(HGDIOBJ(font.0));
                let _ = DeleteDC(dc);
                return Err(GlyphError::InvalidFont);
            }
            if self.devices.len() >= MAX_FACES {
                self.devices.remove(0);
            }
            self.devices.push(Device {
                dc,
                font,
                key: key.clone(),
            });
        }
        Ok(self.devices.last_mut().expect("just pushed"))
    }
}

// SAFETY: a GDI device context and the font selected into it are owned by this
// rasterizer and only used on the thread that holds it. GDI objects are not
// thread-affine for the calls made here (`GetGlyphOutlineW` on a memory DC), and
// the rasterizer is moved to a thread only as a whole, never shared.
unsafe impl Send for GdiRasterizer {}

impl Default for GdiRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemRasterizer for GdiRasterizer {
    fn rasterize(
        &mut self,
        font: &SystemFont,
        glyph: u16,
        options: RasterOptions<'_>,
        _limits: CacheLimits,
        reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        let source = crate::system_face(*font).ok_or(GlyphError::InvalidFont)?;
        let FaceSource::File { path, index } = &source;
        let bytes = std::fs::read(path.as_ref()).map_err(|_| GlyphError::InvalidFont)?;
        let face = describe(&bytes, *index).ok_or(GlyphError::InvalidFont)?;
        self.memory_font(&source, &bytes)?;
        let key = FaceKey {
            family: Arc::from(face.family.as_str()),
            weight: face.weight,
            italic: face.italic,
            pixels: options.size.round().clamp(1.0, 16_384.0) as u32,
            hint: options.hint,
        };
        let device = self.device(&key)?;
        let mut metrics = GLYPHMETRICS::default();
        // Synthetic slant and bold go through the shared outline builder rather
        // than GDI's transform matrix, so Windows and macOS rasterize identical
        // geometry and the tested Linux path covers both.
        let matrix = MAT2 {
            eM11: fixed(1.0),
            eM12: fixed(0.0),
            eM21: fixed(0.0),
            eM22: fixed(1.0),
        };
        let flags = GET_GLYPH_OUTLINE_FORMAT(
            GGO_NATIVE.0
                | GGO_GLYPH_INDEX.0
                | match options.hint {
                    true => 0,
                    false => GGO_UNHINTED.0,
                },
        );
        // SAFETY: the device context and its selected font are live; a null
        // buffer asks GDI for the required byte count only.
        let needed = unsafe {
            GetGlyphOutlineW(
                device.dc,
                u32::from(glyph),
                flags,
                &mut metrics,
                0,
                None,
                &matrix,
            )
        };
        if needed == u32::MAX {
            return Err(GlyphError::UnsupportedGlyph);
        }
        if needed == 0 {
            reserve(0)?;
            return Ok(empty());
        }
        let mut buffer = vec![0u8; needed as usize];
        // SAFETY: the buffer has exactly the size GDI requested and stays live
        // across the call; `metrics` is filled in by GDI.
        let written = unsafe {
            GetGlyphOutlineW(
                device.dc,
                u32::from(glyph),
                flags,
                &mut metrics,
                needed,
                Some(buffer.as_mut_ptr() as *mut c_void),
                &matrix,
            )
        };
        if written == u32::MAX || written != needed {
            return Err(GlyphError::UnsupportedGlyph);
        }
        let mut outline = OutlineBuilder::new(options.size, options.embolden, options.skew);
        read_contours(&buffer, &mut outline)?;
        let offset = [options.offset[0], -options.offset[1]];
        outline.rasterize(offset, reserve)
    }
}

fn empty() -> Image {
    Image {
        placement: Placement::default(),
        content: Content::Mask,
        data: Vec::new().into_boxed_slice(),
    }
}

/// Converts GDI's native outline buffer into `outline` path commands.
///
/// Each polygon header is `cb` bytes long and holds one contour. Line curves
/// list on-curve points; quadratic curves use all but the last point as
/// successive controls, with implied on-curve midpoints between them; cubic
/// curves come in triples of two controls and an endpoint.
fn read_contours(buffer: &[u8], outline: &mut OutlineBuilder) -> Result<(), GlyphError> {
    let mut at = 0usize;
    while at < buffer.len() {
        if buffer.len() - at < size_of::<TTPOLYGONHEADER>() {
            return Err(GlyphError::InvalidFont);
        }
        // SAFETY: the check above guarantees a whole header is in bounds;
        // `TTPOLYGONHEADER` is `repr(C)` over integers and `POINTFX`.
        let header: TTPOLYGONHEADER =
            unsafe { std::ptr::read_unaligned(buffer[at..].as_ptr().cast()) };
        let length = header.cb as usize;
        if length < size_of::<TTPOLYGONHEADER>() || length > buffer.len() - at {
            return Err(GlyphError::InvalidFont);
        }
        outline.move_to(fx(header.pfxStart.x), fx(header.pfxStart.y));
        let mut cursor = at + size_of::<TTPOLYGONHEADER>();
        let end = at + length;
        while cursor < end {
            if end - cursor < 4 {
                return Err(GlyphError::InvalidFont);
            }
            // SAFETY: the tag and count are the first four bytes of a curve and
            // were just bounds-checked.
            let curve_type = u16::from_ne_bytes(buffer[cursor..cursor + 2].try_into().unwrap());
            let count = u16::from_ne_bytes(buffer[cursor + 2..cursor + 4].try_into().unwrap());
            let points = cursor + 4;
            let point_bytes = (count as usize)
                .checked_mul(size_of::<POINTFX>())
                .ok_or(GlyphError::InvalidFont)?;
            if count == 0 || end - points < point_bytes {
                return Err(GlyphError::InvalidFont);
            }
            let point = |index: usize| -> (f32, f32) {
                let offset = points + index * size_of::<POINTFX>();
                // SAFETY: `index < count` and all points fit before `end`.
                let point: POINTFX =
                    unsafe { std::ptr::read_unaligned(buffer[offset..].as_ptr().cast()) };
                (fx(point.x), fx(point.y))
            };
            match u32::from(curve_type) {
                TT_PRIM_LINE => {
                    for index in 0..count as usize {
                        let (x, y) = point(index);
                        outline.line_to(x, y);
                    }
                }
                TT_PRIM_QSPLINE => {
                    if count == 1 {
                        let (x, y) = point(0);
                        outline.line_to(x, y);
                    }
                    for index in 0..count.saturating_sub(1) as usize {
                        let (cx, cy) = point(index);
                        let (x, y) = match index + 1 == count as usize - 1 {
                            true => point(index + 1),
                            false => {
                                let (nx, ny) = point(index + 1);
                                ((cx + nx) / 2.0, (cy + ny) / 2.0)
                            }
                        };
                        outline.quad_to(cx, cy, x, y);
                    }
                }
                TT_PRIM_CSPLINE => {
                    let mut index = 0;
                    while index + 2 < count as usize {
                        let (c1x, c1y) = point(index);
                        let (c2x, c2y) = point(index + 1);
                        let (x, y) = point(index + 2);
                        outline.cubic_to(c1x, c1y, c2x, c2y, x, y);
                        index += 3;
                    }
                }
                _ => return Err(GlyphError::UnsupportedGlyph),
            }
            cursor = points + point_bytes;
        }
        outline.close();
        at = end;
    }
    Ok(())
}

/// Converts a 16.16 `FIXED` to pixels.
fn fx(value: FIXED) -> f32 {
    let raw = (i32::from(value.value) << 16) | i32::from(value.fract);
    raw as f32 / FIXED_ONE
}

fn fixed(value: f32) -> FIXED {
    let raw = (value * FIXED_ONE).round() as i32;
    FIXED {
        fract: (raw & 0xffff) as u16,
        value: (raw >> 16) as i16,
    }
}

/// Registers the GDI rasterizer for [`SystemFont::DWRITE`].
///
/// Called once by `aegle-text`'s DirectWrite backend; returns false if another
/// rasterizer already claimed that backend number.
pub fn register() -> bool {
    register_system_rasterizer(SystemFont::DWRITE, Box::new(GdiRasterizer::new()))
}
