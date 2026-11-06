//! System FreeType rasterizer loaded dynamically at run time.
//!
//! Provides glyph outline and bitmap rendering on Linux and Unix systems
//! using `libfreetype.so.6`.
#![allow(unsafe_code)]

use std::os::raw::{c_char, c_int, c_long, c_short, c_uchar, c_uint, c_ushort, c_void};
use std::sync::OnceLock;

use crate::{CacheLimits, Content, FontData, GlyphError, Image, Placement, RasterOptions};

#[repr(C)]
struct FT_Vector {
    x: c_long,
    y: c_long,
}

#[repr(C)]
struct FT_Matrix {
    xx: c_long,
    xy: c_long,
    yx: c_long,
    yy: c_long,
}

#[repr(C)]
struct FT_Bitmap {
    rows: c_uint,
    width: c_uint,
    pitch: c_int,
    buffer: *mut c_uchar,
    num_grays: c_ushort,
    pixel_mode: c_uchar,
    palette_mode: c_uchar,
    palette: *mut c_void,
}

#[repr(C)]
struct FT_GlyphSlotRec {
    library: *mut c_void,
    face: *mut c_void,
    next: *mut c_void,
    glyph_index: c_uint,
    generic: [c_long; 2],
    metrics: [c_long; 8],
    linear_hori_advance: c_long,
    linear_vert_advance: c_long,
    advance: FT_Vector,
    format: c_uint,
    bitmap: FT_Bitmap,
    bitmap_left: c_int,
    bitmap_top: c_int,
}

/// One fixed strike of a bitmap face; ppem values are 26.6 fixed point.
#[repr(C)]
struct FT_Bitmap_Size {
    height: c_short,
    width: c_short,
    size: c_long,
    x_ppem: c_long,
    y_ppem: c_long,
}

#[repr(C)]
struct FT_Generic {
    data: *mut c_void,
    finalizer: *mut c_void,
}

#[repr(C)]
struct FtFaceRec {
    num_faces: c_long,
    face_index: c_long,
    face_flags: c_long,
    style_flags: c_long,
    num_glyphs: c_long,
    family_name: *mut c_char,
    style_name: *mut c_char,
    num_fixed_sizes: c_int,
    available_sizes: *const FT_Bitmap_Size,
    num_charmaps: c_int,
    charmaps: *mut c_void,
    generic: FT_Generic,
    bbox: [c_long; 4],
    units_per_em: c_ushort,
    ascender: c_short,
    descender: c_short,
    height: c_short,
    max_advance_width: c_short,
    max_advance_height: c_short,
    underline_position: c_short,
    underline_thickness: c_short,
    glyph: *mut FT_GlyphSlotRec,
}

type FtLibrary = *mut c_void;
type FtFace = *mut FtFaceRec;
type FtError = c_int;

/// The face has outlines; without it, only the fixed strikes draw.
const FT_FACE_FLAG_SCALABLE: c_long = 1;
const FT_LOAD_DEFAULT: c_int = 0;
const FT_LOAD_NO_HINTING: c_int = 1 << 1;
/// Ask FreeType for embedded color layers and bitmap strikes.
const FT_LOAD_COLOR: c_int = 1 << 20;
const FT_RENDER_MODE_NORMAL: c_uint = 0;
/// `FT_PIXEL_MODE_GRAY`: one coverage byte per pixel.
const FT_PIXEL_MODE_GRAY: c_uchar = 2;
/// `FT_PIXEL_MODE_BGRA`: premultiplied color bytes, blue first.
const FT_PIXEL_MODE_BGRA: c_uchar = 7;

struct FreeTypeLib {
    _lib: libloading::Library,
    init: unsafe extern "C" fn(*mut FtLibrary) -> FtError,
    done: unsafe extern "C" fn(FtLibrary) -> FtError,
    new_memory_face:
        unsafe extern "C" fn(FtLibrary, *const u8, c_long, c_long, *mut FtFace) -> FtError,
    done_face: unsafe extern "C" fn(FtFace) -> FtError,
    set_pixel_sizes: unsafe extern "C" fn(FtFace, c_uint, c_uint) -> FtError,
    select_size: unsafe extern "C" fn(FtFace, c_int) -> FtError,
    load_glyph: unsafe extern "C" fn(FtFace, c_uint, c_int) -> FtError,
    render_glyph: unsafe extern "C" fn(*mut FT_GlyphSlotRec, c_uint) -> FtError,
    embolden: unsafe extern "C" fn(*mut FT_GlyphSlotRec),
    set_transform: unsafe extern "C" fn(FtFace, *const FT_Matrix, *const FT_Vector),
}

static FT: OnceLock<Option<FreeTypeLib>> = OnceLock::new();

fn ft_lib() -> Option<&'static FreeTypeLib> {
    FT.get_or_init(|| {
        let names = ["libfreetype.so.6", "libfreetype.so"];
        for name in names {
            if let Ok(lib) = unsafe { libloading::Library::new(name) } {
                unsafe {
                    let init = match lib.get(b"FT_Init_FreeType\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let done = match lib.get(b"FT_Done_FreeType\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let new_memory_face = match lib.get(b"FT_New_Memory_Face\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let done_face = match lib.get(b"FT_Done_Face\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let set_pixel_sizes = match lib.get(b"FT_Set_Pixel_Sizes\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let select_size = match lib.get(b"FT_Select_Size\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let load_glyph = match lib.get(b"FT_Load_Glyph\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let render_glyph = match lib.get(b"FT_Render_Glyph\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let embolden = match lib.get(b"FT_GlyphSlot_Embolden\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    let set_transform = match lib.get(b"FT_Set_Transform\0") {
                        Ok(sym) => *sym,
                        Err(_) => continue,
                    };
                    return Some(FreeTypeLib {
                        _lib: lib,
                        init,
                        done,
                        new_memory_face,
                        done_face,
                        set_pixel_sizes,
                        select_size,
                        load_glyph,
                        render_glyph,
                        embolden,
                        set_transform,
                    });
                }
            }
        }
        None
    })
    .as_ref()
}

struct CachedFace {
    blob_id: u64,
    face_index: u32,
    face: FtFace,
}

pub(crate) struct FreeTypeRasterizer {
    library: FtLibrary,
    faces: Vec<CachedFace>,
}

impl FreeTypeRasterizer {
    pub fn new() -> Option<Self> {
        let ft = ft_lib()?;
        let mut library: FtLibrary = std::ptr::null_mut();
        let res = unsafe { (ft.init)(&mut library) };
        if res != 0 || library.is_null() {
            return None;
        }
        Some(Self {
            library,
            faces: Vec::with_capacity(4),
        })
    }

    pub fn rasterize(
        &mut self,
        data: &FontData,
        glyph: u16,
        options: RasterOptions<'_>,
        limits: CacheLimits,
        mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError> {
        let ft = ft_lib().ok_or(GlyphError::InvalidFont)?;
        let blob_id = data.data.id();
        let face_index = data.index;

        let face = if let Some(idx) = self
            .faces
            .iter()
            .position(|f| f.blob_id == blob_id && f.face_index == face_index)
        {
            let item = self.faces.remove(idx);
            self.faces.push(item);
            self.faces.last().unwrap().face
        } else {
            let mut face: FtFace = std::ptr::null_mut();
            let bytes = data.data.data();
            let res = unsafe {
                (ft.new_memory_face)(
                    self.library,
                    bytes.as_ptr(),
                    bytes.len() as c_long,
                    face_index as c_long,
                    &mut face,
                )
            };
            if res != 0 || face.is_null() {
                return Err(GlyphError::InvalidFont);
            }
            if self.faces.len() >= 4 {
                let oldest = self.faces.remove(0);
                unsafe {
                    (ft.done_face)(oldest.face);
                }
            }
            self.faces.push(CachedFace {
                blob_id,
                face_index,
                face,
            });
            face
        };

        unsafe {
            let rec = &*face;
            let scalable = rec.face_flags & FT_FACE_FLAG_SCALABLE != 0;
            // FreeType reports no units per em for a face that is only bitmaps.
            let sized = match scalable {
                true => rec.units_per_em != 0,
                false => rec.num_fixed_sizes > 0 && !rec.available_sizes.is_null(),
            };
            if !sized || (glyph as c_long) >= rec.num_glyphs {
                return Err(GlyphError::InvalidFont);
            }

            // A bitmap-only face (color emoji) draws its nearest strike,
            // which is then resampled to the requested size.
            let scale = match scalable {
                true => {
                    let px = options.size.round().max(1.0) as c_uint;
                    if (ft.set_pixel_sizes)(face, 0, px) != 0 {
                        return Err(GlyphError::InvalidOptions);
                    }
                    1.0
                }
                false => {
                    let sizes = std::slice::from_raw_parts(
                        rec.available_sizes,
                        rec.num_fixed_sizes as usize,
                    );
                    let (index, ppem) = strike(sizes, options.size);
                    if ppem <= 0.0 || (ft.select_size)(face, index) != 0 {
                        return Err(GlyphError::InvalidFont);
                    }
                    options.size / ppem
                }
            };

            let mut matrix = FT_Matrix {
                xx: 0x10000,
                xy: 0,
                yx: 0,
                yy: 0x10000,
            };
            if options.skew != 0 {
                let radians = (f64::from(options.skew)).to_radians();
                let tan = radians.tan();
                matrix.xy = (tan * 65536.0).round() as c_long;
            }

            let delta = FT_Vector {
                x: (f64::from(options.offset[0]) * 64.0).round() as c_long,
                y: (-f64::from(options.offset[1]) * 64.0).round() as c_long,
            };
            (ft.set_transform)(face, &matrix, &delta);

            let mut load_flags = FT_LOAD_DEFAULT | FT_LOAD_COLOR;
            if !options.hint {
                load_flags |= FT_LOAD_NO_HINTING;
            }

            if (ft.load_glyph)(face, glyph as c_uint, load_flags) != 0 {
                return Err(GlyphError::UnsupportedGlyph);
            }

            let slot = rec.glyph;
            if slot.is_null() {
                return Err(GlyphError::UnsupportedGlyph);
            }

            if options.embolden && scalable {
                (ft.embolden)(slot);
            }

            if (ft.render_glyph)(slot, FT_RENDER_MODE_NORMAL) != 0 {
                return Err(GlyphError::UnsupportedGlyph);
            }

            let slot_rec = &*slot;
            let bm = &slot_rec.bitmap;
            let width = bm.width;
            let height = bm.rows;

            if width == 0 || height == 0 {
                reserve(0)?;
                return Ok(Image {
                    placement: Placement::default(),
                    content: Content::Mask,
                    data: Vec::new().into_boxed_slice(),
                });
            }

            let color = match bm.pixel_mode {
                FT_PIXEL_MODE_GRAY => false,
                FT_PIXEL_MODE_BGRA => true,
                _ => return Err(GlyphError::UnsupportedGlyph),
            };
            let channels = if color { 4 } else { 1 };
            let (out_width, out_height) = match scale == 1.0 {
                true => (width, height),
                false => (scaled(width, scale), scaled(height, scale)),
            };
            reserve(out_width as usize * out_height as usize * channels)?;
            let source_bytes = (width as usize) * (height as usize) * channels;
            if source_bytes > limits.bitmap_bytes {
                return Err(GlyphError::BitmapBudget);
            }

            let pitch = bm.pitch;
            let buffer = bm.buffer;
            if buffer.is_null() {
                return Err(GlyphError::UnsupportedGlyph);
            }
            let row_len = width as usize * channels;
            let mut raw = Vec::with_capacity(source_bytes);
            for row in 0..height as usize {
                let row_offset = match pitch >= 0 {
                    true => row * pitch as usize,
                    false => (height as usize - 1 - row) * (-pitch) as usize,
                };
                raw.extend_from_slice(std::slice::from_raw_parts(buffer.add(row_offset), row_len));
            }
            // Resampled while premultiplied, so transparent pixels add no color.
            if (out_width, out_height) != (width, height) {
                raw = resample(&raw, [width, height], [out_width, out_height], channels);
            }
            let out = match color {
                // FreeType composites color layers premultiplied; images are
                // straight sRGB, so divide the color channels by alpha.
                true => {
                    let mut out = Vec::with_capacity(raw.len());
                    for pixel in raw.as_chunks::<4>().0 {
                        let alpha = u32::from(pixel[3]);
                        let straight = |value: u8| {
                            (u32::from(value) * 255)
                                .checked_div(alpha)
                                .map_or(0, |v| v.min(255) as u8)
                        };
                        out.extend_from_slice(&[
                            straight(pixel[2]),
                            straight(pixel[1]),
                            straight(pixel[0]),
                            pixel[3],
                        ]);
                    }
                    out
                }
                false => raw,
            };

            Ok(Image {
                placement: Placement {
                    left: (slot_rec.bitmap_left as f32 * scale).round() as i32,
                    top: (slot_rec.bitmap_top as f32 * scale).round() as i32,
                    width: out_width,
                    height: out_height,
                },
                // A color glyph keeps its own pixels; a mask takes the run color.
                content: match color {
                    true => Content::Color,
                    false => Content::Mask,
                },
                data: out.into_boxed_slice(),
            })
        }
    }

    pub fn clear(&mut self) {
        if let Some(ft) = ft_lib() {
            for f in self.faces.drain(..) {
                unsafe {
                    (ft.done_face)(f.face);
                }
            }
        }
    }
}

/// The strike to draw at `size` pixels per em: the smallest at least that
/// large, else the largest. Returns its index and pixels per em.
fn strike(sizes: &[FT_Bitmap_Size], size: f32) -> (c_int, f32) {
    let ppem = |s: &FT_Bitmap_Size| s.y_ppem as f32 / 64.0;
    let pick = (sizes.iter().enumerate())
        .filter(|(_, s)| ppem(s) >= size)
        .min_by(|a, b| ppem(a.1).total_cmp(&ppem(b.1)))
        .or_else(|| (sizes.iter().enumerate()).max_by(|a, b| ppem(a.1).total_cmp(&ppem(b.1))));
    pick.map_or((0, 0.0), |(i, s)| (i as c_int, ppem(s)))
}

fn scaled(length: c_uint, scale: f32) -> c_uint {
    ((length as f32 * scale).round() as c_uint).max(1)
}

/// Resamples tightly packed pixels from `from` to `to` size, each output
/// pixel the average of the source area it covers.
fn resample(source: &[u8], from: [c_uint; 2], to: [c_uint; 2], channels: usize) -> Vec<u8> {
    // Source pixels and their weights for each output column or row.
    let spans = |from: c_uint, to: c_uint| -> Vec<Vec<(usize, f32)>> {
        let step = from as f32 / to as f32;
        (0..to)
            .map(|o| {
                let (start, end) = (o as f32 * step, ((o + 1) as f32 * step).min(from as f32));
                let first = start.floor() as usize;
                let last = (end.ceil() as usize).min(from as usize);
                (first..last)
                    .map(|i| {
                        let cover = end.min(i as f32 + 1.0) - start.max(i as f32);
                        (i, cover / (end - start))
                    })
                    .filter(|(_, w)| *w > 0.0)
                    .collect()
            })
            .collect()
    };
    let (columns, rows) = (spans(from[0], to[0]), spans(from[1], to[1]));
    let stride = from[0] as usize * channels;
    let mut out = Vec::with_capacity(to[0] as usize * to[1] as usize * channels);
    for row in &rows {
        for column in &columns {
            let mut sum = [0.0f32; 4];
            for &(y, wy) in row {
                for &(x, wx) in column {
                    let at = y * stride + x * channels;
                    for (c, total) in sum.iter_mut().enumerate().take(channels) {
                        *total += f32::from(source[at + c]) * wx * wy;
                    }
                }
            }
            out.extend(sum[..channels].iter().map(|v| v.round().min(255.0) as u8));
        }
    }
    out
}

impl Drop for FreeTypeRasterizer {
    fn drop(&mut self) {
        self.clear();
        if let Some(ft) = ft_lib() {
            unsafe {
                (ft.done)(self.library);
            }
        }
    }
}
