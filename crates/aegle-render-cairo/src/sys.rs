//! The cairo C interface, loaded from the system shared library on first use.
//!
//! Linux distributions ship cairo as `libcairo.so.2`, so nothing is linked at
//! build time: the renderer works with whatever version the distribution has, and
//! a system without cairo reports [`RenderError::BackendUnavailable`] instead of
//! failing to start. Only the small subset needed to rasterize coverage is bound.

#![allow(unsafe_code)]

use std::os::raw::{c_double, c_int, c_uchar, c_void};
use std::sync::OnceLock;

use aegle_render_cpu::RenderError;

/// The library name this backend reports and loads.
pub(crate) const LIBRARY: &str = "libcairo.so.2";

/// `CAIRO_FORMAT_A8`: one byte per pixel, 8-bit coverage.
pub(crate) const FORMAT_A8: c_int = 2;
/// `CAIRO_OPERATOR_SOURCE`: replace the destination.
pub(crate) const OPERATOR_SOURCE: c_int = 1;
/// `CAIRO_ANTIALIAS_DEFAULT`: the backend's balanced antialiasing.
pub(crate) const ANTIALIAS_DEFAULT: c_int = 0;
/// `CAIRO_FILL_RULE_WINDING`.
pub(crate) const FILL_RULE_WINDING: c_int = 0;
/// `CAIRO_FILL_RULE_EVEN_ODD`.
pub(crate) const FILL_RULE_EVEN_ODD: c_int = 1;
/// `CAIRO_LINE_CAP_BUTT`.
pub(crate) const LINE_CAP_BUTT: c_int = 0;
/// `CAIRO_LINE_CAP_ROUND`.
pub(crate) const LINE_CAP_ROUND: c_int = 1;
/// `CAIRO_LINE_CAP_SQUARE`.
pub(crate) const LINE_CAP_SQUARE: c_int = 2;
/// `CAIRO_LINE_JOIN_MITER`.
pub(crate) const LINE_JOIN_MITER: c_int = 0;
/// `CAIRO_LINE_JOIN_ROUND`.
pub(crate) const LINE_JOIN_ROUND: c_int = 1;
/// `CAIRO_LINE_JOIN_BEVEL`.
pub(crate) const LINE_JOIN_BEVEL: c_int = 2;
/// `CAIRO_STATUS_SUCCESS`.
pub(crate) const STATUS_SUCCESS: c_int = 0;

/// A cairo matrix in cairo's `xx, yx, xy, yy, x0, y0` order.
#[repr(C)]
pub(crate) struct Matrix {
    pub(crate) xx: c_double,
    pub(crate) yx: c_double,
    pub(crate) xy: c_double,
    pub(crate) yy: c_double,
    pub(crate) x0: c_double,
    pub(crate) y0: c_double,
}

/// The bound cairo entry points.
pub(crate) struct Cairo {
    /// Keeps the shared library mapped for as long as the function pointers.
    _library: libloading::Library,

    pub(crate) image_surface_create_for_data:
        unsafe extern "C" fn(*mut c_uchar, c_int, c_int, c_int, c_int) -> *mut c_void,
    pub(crate) surface_destroy: unsafe extern "C" fn(*mut c_void),
    pub(crate) surface_status: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub(crate) surface_flush: unsafe extern "C" fn(*mut c_void),
    pub(crate) surface_mark_dirty: unsafe extern "C" fn(*mut c_void),
    pub(crate) create: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub(crate) destroy: unsafe extern "C" fn(*mut c_void),
    pub(crate) status: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub(crate) status_to_string: unsafe extern "C" fn(c_int) -> *const c_uchar,
    pub(crate) save: unsafe extern "C" fn(*mut c_void),
    pub(crate) restore: unsafe extern "C" fn(*mut c_void),
    pub(crate) new_path: unsafe extern "C" fn(*mut c_void),
    pub(crate) set_matrix: unsafe extern "C" fn(*mut c_void, *const Matrix),
    pub(crate) move_to: unsafe extern "C" fn(*mut c_void, c_double, c_double),
    pub(crate) line_to: unsafe extern "C" fn(*mut c_void, c_double, c_double),
    pub(crate) curve_to: unsafe extern "C" fn(
        *mut c_void,
        c_double,
        c_double,
        c_double,
        c_double,
        c_double,
        c_double,
    ),
    pub(crate) close_path: unsafe extern "C" fn(*mut c_void),
    pub(crate) set_fill_rule: unsafe extern "C" fn(*mut c_void, c_int),
    pub(crate) fill: unsafe extern "C" fn(*mut c_void),
    pub(crate) stroke: unsafe extern "C" fn(*mut c_void),
    pub(crate) set_antialias: unsafe extern "C" fn(*mut c_void, c_int),
    pub(crate) set_tolerance: unsafe extern "C" fn(*mut c_void, c_double),
    pub(crate) set_operator: unsafe extern "C" fn(*mut c_void, c_int),
    pub(crate) set_source_rgba:
        unsafe extern "C" fn(*mut c_void, c_double, c_double, c_double, c_double),
    pub(crate) set_line_width: unsafe extern "C" fn(*mut c_void, c_double),
    pub(crate) set_line_cap: unsafe extern "C" fn(*mut c_void, c_int),
    pub(crate) set_line_join: unsafe extern "C" fn(*mut c_void, c_int),
    pub(crate) set_miter_limit: unsafe extern "C" fn(*mut c_void, c_double),
    pub(crate) clip: unsafe extern "C" fn(*mut c_void),
    pub(crate) reset_clip: unsafe extern "C" fn(*mut c_void),
    pub(crate) rectangle: unsafe extern "C" fn(*mut c_void, c_double, c_double, c_double, c_double),
    pub(crate) version_string: unsafe extern "C" fn() -> *const c_uchar,
}

static CAIRO: OnceLock<Option<Cairo>> = OnceLock::new();

/// The system cairo entry points, or `None` when the library is not installed.
pub(crate) fn cairo() -> Option<&'static Cairo> {
    CAIRO.get_or_init(load).as_ref()
}

/// Fails with [`RenderError::BackendUnavailable`] when cairo is missing.
pub(crate) fn require() -> Result<&'static Cairo, RenderError> {
    cairo().ok_or(RenderError::BackendUnavailable { library: LIBRARY })
}

/// The loaded cairo version, or `None` when the library is not installed.
pub fn version() -> Option<&'static str> {
    let cairo = cairo()?;
    // SAFETY: cairo_version_string returns a static NUL-terminated string that
    // outlives the library mapping kept by the same `Cairo` value.
    let pointer = unsafe { (cairo.version_string)() };
    if pointer.is_null() {
        return None;
    }
    let bytes = unsafe { std::ffi::CStr::from_ptr(pointer as *const _) };
    bytes.to_str().ok()
}

fn load() -> Option<Cairo> {
    // SAFETY: loading a shared library is unsafe only because its initializers run
    // and because the symbol types below are asserted by this module. Every symbol
    // is bound at once; a library missing any of them is rejected as a whole, so
    // the struct is never partially initialized.
    unsafe {
        let library = libloading::Library::new(LIBRARY)
            .or_else(|_| libloading::Library::new("libcairo.so"))
            .ok()?;
        let symbol = |name: &[u8]| library.get::<*mut c_void>(name).ok().map(|s| *s);
        macro_rules! bind {
            ($name:literal : $type:ty) => {
                std::mem::transmute::<*mut c_void, $type>(symbol(concat!($name, "\0").as_bytes())?)
            };
        }
        let cairo = Cairo {
            image_surface_create_for_data: bind!("cairo_image_surface_create_for_data"
                : unsafe extern "C" fn(*mut c_uchar, c_int, c_int, c_int, c_int) -> *mut c_void),
            surface_destroy: bind!("cairo_surface_destroy" : unsafe extern "C" fn(*mut c_void)),
            surface_status: bind!("cairo_surface_status" : unsafe extern "C" fn(*mut c_void) -> c_int),
            surface_flush: bind!("cairo_surface_flush" : unsafe extern "C" fn(*mut c_void)),
            surface_mark_dirty: bind!("cairo_surface_mark_dirty" : unsafe extern "C" fn(*mut c_void)),
            create: bind!("cairo_create" : unsafe extern "C" fn(*mut c_void) -> *mut c_void),
            destroy: bind!("cairo_destroy" : unsafe extern "C" fn(*mut c_void)),
            status: bind!("cairo_status" : unsafe extern "C" fn(*mut c_void) -> c_int),
            status_to_string: bind!("cairo_status_to_string" : unsafe extern "C" fn(c_int) -> *const c_uchar),
            save: bind!("cairo_save" : unsafe extern "C" fn(*mut c_void)),
            restore: bind!("cairo_restore" : unsafe extern "C" fn(*mut c_void)),
            new_path: bind!("cairo_new_path" : unsafe extern "C" fn(*mut c_void)),
            set_matrix: bind!("cairo_set_matrix" : unsafe extern "C" fn(*mut c_void, *const Matrix)),
            move_to: bind!("cairo_move_to" : unsafe extern "C" fn(*mut c_void, c_double, c_double)),
            line_to: bind!("cairo_line_to" : unsafe extern "C" fn(*mut c_void, c_double, c_double)),
            curve_to: bind!("cairo_curve_to" : unsafe extern "C" fn(*mut c_void, c_double, c_double, c_double, c_double, c_double, c_double)),
            close_path: bind!("cairo_close_path" : unsafe extern "C" fn(*mut c_void)),
            set_fill_rule: bind!("cairo_set_fill_rule" : unsafe extern "C" fn(*mut c_void, c_int)),
            fill: bind!("cairo_fill" : unsafe extern "C" fn(*mut c_void)),
            stroke: bind!("cairo_stroke" : unsafe extern "C" fn(*mut c_void)),
            set_antialias: bind!("cairo_set_antialias" : unsafe extern "C" fn(*mut c_void, c_int)),
            set_tolerance: bind!("cairo_set_tolerance" : unsafe extern "C" fn(*mut c_void, c_double)),
            set_operator: bind!("cairo_set_operator" : unsafe extern "C" fn(*mut c_void, c_int)),
            set_source_rgba: bind!("cairo_set_source_rgba" : unsafe extern "C" fn(*mut c_void, c_double, c_double, c_double, c_double)),
            set_line_width: bind!("cairo_set_line_width" : unsafe extern "C" fn(*mut c_void, c_double)),
            set_line_cap: bind!("cairo_set_line_cap" : unsafe extern "C" fn(*mut c_void, c_int)),
            set_line_join: bind!("cairo_set_line_join" : unsafe extern "C" fn(*mut c_void, c_int)),
            set_miter_limit: bind!("cairo_set_miter_limit" : unsafe extern "C" fn(*mut c_void, c_double)),
            clip: bind!("cairo_clip" : unsafe extern "C" fn(*mut c_void)),
            reset_clip: bind!("cairo_reset_clip" : unsafe extern "C" fn(*mut c_void)),
            rectangle: bind!("cairo_rectangle" : unsafe extern "C" fn(*mut c_void, c_double, c_double, c_double, c_double)),
            version_string: bind!("cairo_version_string" : unsafe extern "C" fn() -> *const c_uchar),
            _library: library,
        };
        Some(cairo)
    }
}
