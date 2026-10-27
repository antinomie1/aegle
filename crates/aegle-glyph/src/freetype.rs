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
    available_sizes: *mut c_void,
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
            if rec.units_per_em == 0 || (glyph as c_long) >= rec.num_glyphs {
                return Err(GlyphError::InvalidFont);
            }

            let px = options.size.round().max(1.0) as c_uint;
            if (ft.set_pixel_sizes)(face, 0, px) != 0 {
                return Err(GlyphError::InvalidOptions);
            }

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

            if options.embolden {
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
            let pixels = (width as usize) * (height as usize);
            let bytes = pixels * channels;
            reserve(bytes)?;
            if bytes > limits.bitmap_bytes {
                return Err(GlyphError::BitmapBudget);
            }

            let pitch = bm.pitch;
            let buffer = bm.buffer;
            if buffer.is_null() {
                return Err(GlyphError::UnsupportedGlyph);
            }
            let row_len = width as usize * channels;
            let mut out = Vec::with_capacity(bytes);
            for row in 0..height as usize {
                let row_offset = match pitch >= 0 {
                    true => row * pitch as usize,
                    false => (height as usize - 1 - row) * (-pitch) as usize,
                };
                let row_slice = std::slice::from_raw_parts(buffer.add(row_offset), row_len);
                match color {
                    // FreeType composites color layers premultiplied; images are
                    // straight sRGB, so divide the color channels by alpha.
                    true => {
                        for pixel in row_slice.as_chunks::<4>().0 {
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
                    }
                    false => out.extend_from_slice(row_slice),
                }
            }

            Ok(Image {
                placement: Placement {
                    left: slot_rec.bitmap_left,
                    top: slot_rec.bitmap_top,
                    width,
                    height,
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
