//! The Pango, PangoFT2, GObject and Fontconfig C interfaces, loaded on first use.
//!
//! Linux distributions ship these as versioned shared objects, so nothing is
//! linked at build time. A missing library or export is remembered as "no system
//! text stack" instead of being retried.
#![allow(unsafe_code)]

use std::os::raw::{c_char, c_double, c_int, c_uint, c_void};
use std::sync::OnceLock;

pub(super) const PANGO_SCALE: f64 = 1024.0;
pub(super) const PANGO_WRAP_WORD_CHAR: c_int = 2;
pub(super) const PANGO_ALIGN_LEFT: c_int = 0;
pub(super) const PANGO_ALIGN_CENTER: c_int = 1;
pub(super) const PANGO_ALIGN_RIGHT: c_int = 2;
pub(super) const PANGO_UNDERLINE_SINGLE: c_int = 1;
/// A zero-width glyph Pango inserts for clusters it does not draw.
pub(super) const PANGO_GLYPH_EMPTY: c_uint = 0x0FFF_FFFF;
/// The high bit of glyphs that stand for text no font could map.
pub(super) const PANGO_GLYPH_UNKNOWN_FLAG: c_uint = 0x1000_0000;

#[repr(C)]
pub(super) struct PangoRectangle {
    pub(super) x: c_int,
    pub(super) y: c_int,
    pub(super) width: c_int,
    pub(super) height: c_int,
}

#[repr(C)]
pub(super) struct PangoGlyphGeometry {
    pub(super) width: c_int,
    pub(super) x_offset: c_int,
    pub(super) y_offset: c_int,
}

#[repr(C)]
pub(super) struct PangoGlyphInfo {
    pub(super) glyph: c_uint,
    pub(super) geometry: PangoGlyphGeometry,
    pub(super) attr: u32,
}

#[repr(C)]
pub(super) struct PangoGlyphString {
    pub(super) num_glyphs: c_int,
    pub(super) glyphs: *mut PangoGlyphInfo,
    pub(super) log_clusters: *mut c_int,
    pub(super) space: c_int,
}

#[repr(C)]
pub(super) struct PangoAnalysis {
    pub(super) shape_engine: *mut c_void,
    pub(super) lang_engine: *mut c_void,
    pub(super) font: *mut c_void,
    pub(super) level: u8,
    pub(super) gravity: u8,
    pub(super) flags: u8,
    pub(super) script: u8,
    pub(super) language: *mut c_void,
    pub(super) extra_attrs: *mut c_void,
}

#[repr(C)]
pub(super) struct PangoItem {
    pub(super) offset: c_int,
    pub(super) length: c_int,
    pub(super) num_chars: c_int,
    pub(super) analysis: PangoAnalysis,
}

#[repr(C)]
pub(super) struct PangoGlyphItem {
    pub(super) item: *mut PangoItem,
    pub(super) glyphs: *mut PangoGlyphString,
}

#[repr(C)]
pub(super) struct PangoLayoutLine {
    pub(super) layout: *mut c_void,
    pub(super) start_index: c_int,
    pub(super) length: c_int,
    pub(super) runs: *mut c_void,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct PangoLogAttr {
    pub(super) raw: u32,
}

impl PangoLogAttr {
    pub(super) fn is_cursor_position(&self) -> bool {
        (self.raw & (1 << 4)) != 0
    }
}

pub(super) struct PangoLib {
    pub(super) _pango: libloading::Library,
    pub(super) _pangoft2: libloading::Library,
    pub(super) _gobject: libloading::Library,
    pub(super) _fc: libloading::Library,

    pub(super) font_description_new: unsafe extern "C" fn() -> *mut c_void,
    pub(super) font_description_free: unsafe extern "C" fn(*mut c_void),
    pub(super) font_description_set_family: unsafe extern "C" fn(*mut c_void, *const c_char),
    pub(super) font_description_set_absolute_size: unsafe extern "C" fn(*mut c_void, c_double),
    pub(super) font_description_set_weight: unsafe extern "C" fn(*mut c_void, c_int),
    pub(super) font_description_set_style: unsafe extern "C" fn(*mut c_void, c_int),

    pub(super) layout_new: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub(super) layout_set_text: unsafe extern "C" fn(*mut c_void, *const c_char, c_int),
    pub(super) layout_set_font_description: unsafe extern "C" fn(*mut c_void, *const c_void),
    pub(super) layout_set_width: unsafe extern "C" fn(*mut c_void, c_int),
    pub(super) layout_set_wrap: unsafe extern "C" fn(*mut c_void, c_int),
    pub(super) layout_set_alignment: unsafe extern "C" fn(*mut c_void, c_int),
    pub(super) layout_get_size: unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int),
    pub(super) layout_get_pixel_size: unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int),
    pub(super) layout_get_baseline: unsafe extern "C" fn(*mut c_void) -> c_int,
    #[allow(dead_code)]
    pub(super) layout_get_line_count: unsafe extern "C" fn(*mut c_void) -> c_int,

    pub(super) layout_get_iter: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub(super) layout_iter_free: unsafe extern "C" fn(*mut c_void),
    pub(super) layout_iter_next_run: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub(super) layout_iter_next_line: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub(super) layout_iter_get_run_readonly:
        unsafe extern "C" fn(*mut c_void) -> *mut PangoGlyphItem,
    pub(super) layout_iter_get_line_readonly:
        unsafe extern "C" fn(*mut c_void) -> *mut PangoLayoutLine,
    pub(super) layout_iter_get_baseline: unsafe extern "C" fn(*mut c_void) -> c_int,
    #[allow(dead_code)]
    pub(super) layout_iter_get_run_extents:
        unsafe extern "C" fn(*mut c_void, *mut PangoRectangle, *mut PangoRectangle),
    pub(super) layout_iter_get_line_extents:
        unsafe extern "C" fn(*mut c_void, *mut PangoRectangle, *mut PangoRectangle),
    pub(super) layout_line_get_extents:
        unsafe extern "C" fn(*mut PangoLayoutLine, *mut PangoRectangle, *mut PangoRectangle),

    pub(super) layout_xy_to_index:
        unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut c_int, *mut c_int) -> c_int,
    pub(super) layout_index_to_pos: unsafe extern "C" fn(*mut c_void, c_int, *mut PangoRectangle),
    pub(super) layout_get_cursor_pos:
        unsafe extern "C" fn(*mut c_void, c_int, *mut PangoRectangle, *mut PangoRectangle),
    pub(super) layout_move_cursor_visually:
        unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, c_int, *mut c_int, *mut c_int),
    pub(super) get_log_attrs:
        unsafe extern "C" fn(*const c_char, c_int, c_int, *mut c_void, *mut PangoLogAttr, c_int),

    pub(super) attr_list_new: unsafe extern "C" fn() -> *mut c_void,
    pub(super) attr_list_unref: unsafe extern "C" fn(*mut c_void),
    pub(super) attr_list_insert: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub(super) attr_underline_new: unsafe extern "C" fn(c_int) -> *mut c_void,
    pub(super) attr_letter_spacing_new: unsafe extern "C" fn(c_int) -> *mut c_void,
    pub(super) layout_set_attributes: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub(super) layout_set_line_spacing: unsafe extern "C" fn(*mut c_void, f32),

    pub(super) ft2_font_map_new: unsafe extern "C" fn() -> *mut c_void,
    pub(super) ft2_font_map_set_resolution: unsafe extern "C" fn(*mut c_void, c_double, c_double),
    pub(super) font_map_create_context: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    #[allow(dead_code)]
    pub(super) fc_font_get_pattern: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub(super) pango_fc_font_map_set_config: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub(super) pango_fc_font_map_config_changed: unsafe extern "C" fn(*mut c_void),
    pub(super) g_object_unref: unsafe extern "C" fn(*mut c_void),

    pub(super) fc_init: unsafe extern "C" fn() -> c_int,
    pub(super) fc_config_create: unsafe extern "C" fn() -> *mut c_void,
    pub(super) fc_config_destroy: unsafe extern "C" fn(*mut c_void),
    pub(super) fc_config_app_font_add_file:
        unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    pub(super) fc_config_get_fonts: unsafe extern "C" fn(*mut c_void, c_int) -> *mut FcFontSet,
    pub(super) pango_fc_font_map_get_config: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    #[allow(dead_code)]
    pub(super) fc_pattern_get_string:
        unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut *mut c_char) -> c_int,
    #[allow(dead_code)]
    pub(super) fc_pattern_get_integer:
        unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_int) -> c_int,
}

/// `FcSetApplication`: the fonts added with `FcConfigAppFontAddFile`.
pub(super) const FC_SET_APPLICATION: c_int = 1;

#[repr(C)]
pub(super) struct FcFontSet {
    pub(super) nfont: c_int,
    pub(super) sfont: c_int,
    pub(super) fonts: *mut *mut c_void,
}

static PANGO: OnceLock<Option<PangoLib>> = OnceLock::new();

pub(super) fn pango() -> Option<&'static PangoLib> {
    PANGO
        .get_or_init(|| unsafe {
            let pango = match libloading::Library::new("libpango-1.0.so.0")
                .or_else(|_| libloading::Library::new("libpango-1.0.so"))
            {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("FAIL load pango: {}", e);
                    return None;
                }
            };
            let pangoft2 = match libloading::Library::new("libpangoft2-1.0.so.0")
                .or_else(|_| libloading::Library::new("libpangoft2-1.0.so"))
            {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("FAIL load pangoft2: {}", e);
                    return None;
                }
            };
            let gobject = match libloading::Library::new("libgobject-2.0.so.0")
                .or_else(|_| libloading::Library::new("libgobject-2.0.so"))
            {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("FAIL load gobject: {}", e);
                    return None;
                }
            };
            let fc = match libloading::Library::new("libfontconfig.so.1")
                .or_else(|_| libloading::Library::new("libfontconfig.so"))
            {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("FAIL load fc: {}", e);
                    return None;
                }
            };

            macro_rules! get {
                ($lib:expr, $name:expr) => {
                    match $lib.get($name) {
                        Ok(sym) => *sym,
                        Err(e) => {
                            eprintln!("FAIL symbol {:?}: {}", std::str::from_utf8($name), e);
                            return None;
                        }
                    }
                };
            }

            Some(PangoLib {
                font_description_new: get!(pango, b"pango_font_description_new\0"),
                font_description_free: get!(pango, b"pango_font_description_free\0"),
                font_description_set_family: get!(pango, b"pango_font_description_set_family\0"),
                font_description_set_absolute_size: get!(
                    pango,
                    b"pango_font_description_set_absolute_size\0"
                ),
                font_description_set_weight: get!(pango, b"pango_font_description_set_weight\0"),
                font_description_set_style: get!(pango, b"pango_font_description_set_style\0"),

                layout_new: get!(pango, b"pango_layout_new\0"),
                layout_set_text: get!(pango, b"pango_layout_set_text\0"),
                layout_set_font_description: get!(pango, b"pango_layout_set_font_description\0"),
                layout_set_width: get!(pango, b"pango_layout_set_width\0"),
                layout_set_wrap: get!(pango, b"pango_layout_set_wrap\0"),
                layout_set_alignment: get!(pango, b"pango_layout_set_alignment\0"),
                layout_get_size: get!(pango, b"pango_layout_get_size\0"),
                layout_get_pixel_size: get!(pango, b"pango_layout_get_pixel_size\0"),
                layout_get_baseline: get!(pango, b"pango_layout_get_baseline\0"),
                layout_get_line_count: get!(pango, b"pango_layout_get_line_count\0"),

                layout_get_iter: get!(pango, b"pango_layout_get_iter\0"),
                layout_iter_free: get!(pango, b"pango_layout_iter_free\0"),
                layout_iter_next_run: get!(pango, b"pango_layout_iter_next_run\0"),
                layout_iter_next_line: get!(pango, b"pango_layout_iter_next_line\0"),
                layout_iter_get_run_readonly: get!(pango, b"pango_layout_iter_get_run_readonly\0"),
                layout_iter_get_line_readonly: get!(
                    pango,
                    b"pango_layout_iter_get_line_readonly\0"
                ),
                layout_iter_get_baseline: get!(pango, b"pango_layout_iter_get_baseline\0"),
                layout_iter_get_run_extents: get!(pango, b"pango_layout_iter_get_run_extents\0"),
                layout_iter_get_line_extents: get!(pango, b"pango_layout_iter_get_line_extents\0"),
                layout_line_get_extents: get!(pango, b"pango_layout_line_get_extents\0"),

                layout_xy_to_index: get!(pango, b"pango_layout_xy_to_index\0"),
                layout_index_to_pos: get!(pango, b"pango_layout_index_to_pos\0"),
                layout_get_cursor_pos: get!(pango, b"pango_layout_get_cursor_pos\0"),
                layout_move_cursor_visually: get!(pango, b"pango_layout_move_cursor_visually\0"),
                get_log_attrs: get!(pango, b"pango_get_log_attrs\0"),

                attr_list_new: get!(pango, b"pango_attr_list_new\0"),
                attr_list_unref: get!(pango, b"pango_attr_list_unref\0"),
                attr_list_insert: get!(pango, b"pango_attr_list_insert\0"),
                attr_underline_new: get!(pango, b"pango_attr_underline_new\0"),
                attr_letter_spacing_new: get!(pango, b"pango_attr_letter_spacing_new\0"),
                layout_set_attributes: get!(pango, b"pango_layout_set_attributes\0"),
                layout_set_line_spacing: get!(pango, b"pango_layout_set_line_spacing\0"),

                ft2_font_map_new: get!(pangoft2, b"pango_ft2_font_map_new\0"),
                ft2_font_map_set_resolution: get!(pangoft2, b"pango_ft2_font_map_set_resolution\0"),
                font_map_create_context: get!(pango, b"pango_font_map_create_context\0"),
                fc_font_get_pattern: get!(pangoft2, b"pango_fc_font_get_pattern\0"),
                pango_fc_font_map_set_config: get!(pangoft2, b"pango_fc_font_map_set_config\0"),
                pango_fc_font_map_config_changed: get!(
                    pangoft2,
                    b"pango_fc_font_map_config_changed\0"
                ),
                g_object_unref: get!(gobject, b"g_object_unref\0"),

                fc_init: get!(fc, b"FcInit\0"),
                fc_config_create: get!(fc, b"FcConfigCreate\0"),
                fc_config_destroy: get!(fc, b"FcConfigDestroy\0"),
                fc_config_app_font_add_file: get!(fc, b"FcConfigAppFontAddFile\0"),
                fc_config_get_fonts: get!(fc, b"FcConfigGetFonts\0"),
                pango_fc_font_map_get_config: get!(pangoft2, b"pango_fc_font_map_get_config\0"),
                fc_pattern_get_string: get!(fc, b"FcPatternGetString\0"),
                fc_pattern_get_integer: get!(fc, b"FcPatternGetInteger\0"),

                _pango: pango,
                _pangoft2: pangoft2,
                _gobject: gobject,
                _fc: fc,
            })
        })
        .as_ref()
}
