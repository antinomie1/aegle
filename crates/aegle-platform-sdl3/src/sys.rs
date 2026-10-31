//! The SDL3 C interfaces this backend uses, loaded on first use.
//!
//! Distribution packages ship SDL3 as a versioned shared object, so nothing is
//! linked at build time and no header is read. Only the entry points named here
//! are bound; a library missing any of them is rejected as a whole, so a
//! partially initialized table is never used.
#![allow(unsafe_code)]

use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    fmt,
    sync::OnceLock,
};

/// `SDL_INIT_VIDEO`.
pub(crate) const INIT_VIDEO: u32 = 0x0000_0020;

/// `SDL_WINDOW_HIDDEN`.
pub(crate) const WINDOW_HIDDEN: u64 = 0x0000_0000_0000_0008;
/// `SDL_WINDOW_RESIZABLE`.
pub(crate) const WINDOW_RESIZABLE: u64 = 0x0000_0000_0000_0020;
/// `SDL_WINDOW_MINIMIZED`.
pub(crate) const WINDOW_MINIMIZED: u64 = 0x0000_0000_0000_0040;
/// `SDL_WINDOW_INPUT_FOCUS`.
pub(crate) const WINDOW_INPUT_FOCUS: u64 = 0x0000_0000_0000_0200;
/// `SDL_WINDOW_HIGH_PIXEL_DENSITY`.
pub(crate) const WINDOW_HIGH_PIXEL_DENSITY: u64 = 0x0000_0000_0000_2000;

/// `SDL_SYSTEM_THEME_UNKNOWN`.
pub(crate) const THEME_UNKNOWN: c_int = 0;
/// `SDL_SYSTEM_THEME_DARK`.
pub(crate) const THEME_DARK: c_int = 2;

/// `SDL_PIXELFORMAT_XRGB8888`: memory order B, G, R, X on little-endian.
pub(crate) const PIXELFORMAT_XRGB8888: u32 = 0x1616_1804;
/// `SDL_PIXELFORMAT_ARGB8888`: memory order B, G, R, A on little-endian, the
/// byte order the contract's BGRA8 buffers already have.
pub(crate) const PIXELFORMAT_ARGB8888: u32 = 0x1636_2004;

/// `SDL_TOUCH_MOUSEID`: synthetic mouse events carrying a finger's contact.
pub(crate) const TOUCH_MOUSEID: u32 = u32::MAX;

// Event types.
pub(crate) const EVENT_QUIT: u32 = 0x100;
pub(crate) const EVENT_SYSTEM_THEME_CHANGED: u32 = 0x108;
pub(crate) const EVENT_WINDOW_SHOWN: u32 = 0x202;
pub(crate) const EVENT_WINDOW_HIDDEN: u32 = 0x203;
pub(crate) const EVENT_WINDOW_EXPOSED: u32 = 0x204;
pub(crate) const EVENT_WINDOW_RESIZED: u32 = 0x206;
pub(crate) const EVENT_WINDOW_PIXEL_SIZE_CHANGED: u32 = 0x207;
pub(crate) const EVENT_WINDOW_MINIMIZED: u32 = 0x209;
pub(crate) const EVENT_WINDOW_MAXIMIZED: u32 = 0x20a;
pub(crate) const EVENT_WINDOW_RESTORED: u32 = 0x20b;
pub(crate) const EVENT_WINDOW_MOUSE_ENTER: u32 = 0x20c;
pub(crate) const EVENT_WINDOW_MOUSE_LEAVE: u32 = 0x20d;
pub(crate) const EVENT_WINDOW_FOCUS_GAINED: u32 = 0x20e;
pub(crate) const EVENT_WINDOW_FOCUS_LOST: u32 = 0x20f;
pub(crate) const EVENT_WINDOW_CLOSE_REQUESTED: u32 = 0x210;
pub(crate) const EVENT_WINDOW_DISPLAY_CHANGED: u32 = 0x213;
pub(crate) const EVENT_WINDOW_DISPLAY_SCALE_CHANGED: u32 = 0x214;
pub(crate) const EVENT_WINDOW_OCCLUDED: u32 = 0x216;
pub(crate) const EVENT_WINDOW_DESTROYED: u32 = 0x219;
pub(crate) const EVENT_WINDOW_ENTER_FULLSCREEN: u32 = 0x217;
pub(crate) const EVENT_WINDOW_LEAVE_FULLSCREEN: u32 = 0x218;
pub(crate) const EVENT_KEY_DOWN: u32 = 0x300;
pub(crate) const EVENT_KEY_UP: u32 = 0x301;
pub(crate) const EVENT_TEXT_EDITING: u32 = 0x302;
pub(crate) const EVENT_TEXT_INPUT: u32 = 0x303;
pub(crate) const EVENT_KEYMAP_CHANGED: u32 = 0x304;
pub(crate) const EVENT_MOUSE_MOTION: u32 = 0x400;
pub(crate) const EVENT_MOUSE_BUTTON_DOWN: u32 = 0x401;
pub(crate) const EVENT_MOUSE_BUTTON_UP: u32 = 0x402;
pub(crate) const EVENT_MOUSE_WHEEL: u32 = 0x403;
pub(crate) const EVENT_FINGER_DOWN: u32 = 0x700;
pub(crate) const EVENT_FINGER_UP: u32 = 0x701;
pub(crate) const EVENT_FINGER_MOTION: u32 = 0x702;
pub(crate) const EVENT_FINGER_CANCELED: u32 = 0x703;
pub(crate) const EVENT_DROP_FILE: u32 = 0x1000;
pub(crate) const EVENT_DROP_TEXT: u32 = 0x1001;
pub(crate) const EVENT_DROP_BEGIN: u32 = 0x1002;
pub(crate) const EVENT_DROP_COMPLETE: u32 = 0x1003;
pub(crate) const EVENT_DROP_POSITION: u32 = 0x1004;

/// `SDL_Window`.
#[repr(C)]
pub(crate) struct Window {
    _private: [u8; 0],
}

/// `SDL_Cursor`.
#[repr(C)]
pub(crate) struct Cursor {
    _private: [u8; 0],
}

/// `SDL_Rect`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Rect {
    pub x: c_int,
    pub y: c_int,
    pub w: c_int,
    pub h: c_int,
}

/// `SDL_Surface`.
#[repr(C)]
pub(crate) struct Surface {
    pub flags: u32,
    pub format: u32,
    pub w: c_int,
    pub h: c_int,
    pub pitch: c_int,
    pub pixels: *mut c_void,
    pub refcount: c_int,
    pub reserved: *mut c_void,
}

/// `SDL_DisplayMode`.
#[repr(C)]
pub(crate) struct DisplayMode {
    pub display_id: u32,
    pub format: u32,
    pub w: c_int,
    pub h: c_int,
    pub pixel_density: f32,
    pub refresh_rate: f32,
    pub refresh_rate_numerator: c_int,
    pub refresh_rate_denominator: c_int,
    pub internal: *mut c_void,
}

/// `SDL_UserEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct UserEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub code: i32,
    pub data1: *mut c_void,
    pub data2: *mut c_void,
}

/// `SDL_WindowEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct WindowEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub data1: i32,
    pub data2: i32,
}

/// `SDL_KeyboardEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct KeyboardEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub which: u32,
    pub scancode: i32,
    pub key: u32,
    pub modifiers: u16,
    pub raw: u16,
    pub down: u8,
    pub repeat: u8,
}

/// `SDL_TextEditingEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TextEditingEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub text: *const c_char,
    pub start: i32,
    pub length: i32,
}

/// `SDL_TextInputEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TextInputEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub text: *const c_char,
}

/// `SDL_MouseMotionEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct MouseMotionEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub which: u32,
    pub state: u32,
    pub x: f32,
    pub y: f32,
    pub xrel: f32,
    pub yrel: f32,
}

/// `SDL_MouseButtonEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct MouseButtonEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub which: u32,
    pub button: u8,
    pub down: u8,
    pub clicks: u8,
    pub padding: u8,
    pub x: f32,
    pub y: f32,
}

/// `SDL_MouseWheelEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct MouseWheelEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub which: u32,
    pub x: f32,
    pub y: f32,
    pub direction: c_int,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub integer_x: i32,
    pub integer_y: i32,
}

/// `SDL_TouchFingerEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TouchFingerEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub touch_id: u64,
    pub finger_id: u64,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub pressure: f32,
    pub window_id: u32,
}

/// `SDL_DropEvent`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct DropEvent {
    pub kind: u32,
    pub reserved: u32,
    pub timestamp: u64,
    pub window_id: u32,
    pub x: f32,
    pub y: f32,
    pub source: *const c_char,
    pub data: *const c_char,
}

/// `SDL_Event`, a union of the structures above with 128 bytes of padding.
#[repr(C)]
pub(crate) union Event {
    pub kind: u32,
    pub window: WindowEvent,
    pub key: KeyboardEvent,
    pub edit: TextEditingEvent,
    pub text: TextInputEvent,
    pub motion: MouseMotionEvent,
    pub button: MouseButtonEvent,
    pub wheel: MouseWheelEvent,
    pub finger: TouchFingerEvent,
    pub drop: DropEvent,
    pub user: UserEvent,
    pub padding: [u8; 128],
}

impl Event {
    /// A zeroed event; every field is an integer, float or pointer.
    pub(crate) fn zeroed() -> Self {
        // SAFETY: all union fields are plain data with a valid all-zero bit
        // pattern (null pointers, zero integers and floats).
        unsafe { std::mem::zeroed() }
    }

    /// A user event of `kind`, the shape [`Lib::push_event`] accepts.
    pub(crate) fn user(kind: u32) -> Self {
        let mut event = Self::zeroed();
        event.user = UserEvent {
            kind,
            reserved: 0,
            timestamp: 0,
            window_id: 0,
            code: 0,
            data1: std::ptr::null_mut(),
            data2: std::ptr::null_mut(),
        };
        event
    }
}

// SAFETY: the union holds only plain integers, floats and pointers; it owns
// nothing and has no drop glue.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

/// The bound SDL3 entry points and the library that owns them.
pub(crate) struct Lib {
    pub init: unsafe extern "C" fn(u32) -> bool,
    pub quit: unsafe extern "C" fn(),
    pub get_error: unsafe extern "C" fn() -> *const c_char,
    pub get_current_video_driver: unsafe extern "C" fn() -> *const c_char,
    pub register_events: unsafe extern "C" fn(c_int) -> u32,
    pub poll_event: unsafe extern "C" fn(*mut Event) -> bool,
    pub wait_event_timeout: unsafe extern "C" fn(*mut Event, c_int) -> bool,
    pub push_event: unsafe extern "C" fn(*const Event) -> bool,
    pub create_window: unsafe extern "C" fn(*const c_char, c_int, c_int, u64) -> *mut Window,
    pub destroy_window: unsafe extern "C" fn(*mut Window),
    pub show_window: unsafe extern "C" fn(*mut Window) -> bool,
    pub set_window_title: unsafe extern "C" fn(*mut Window, *const c_char) -> bool,
    pub get_window_id: unsafe extern "C" fn(*mut Window) -> u32,
    pub get_window_flags: unsafe extern "C" fn(*mut Window) -> u64,
    pub get_window_size: unsafe extern "C" fn(*mut Window, *mut c_int, *mut c_int) -> bool,
    pub get_window_size_in_pixels:
        unsafe extern "C" fn(*mut Window, *mut c_int, *mut c_int) -> bool,
    pub get_window_display_scale: unsafe extern "C" fn(*mut Window) -> f32,
    /// Bound for the `gpu` feature's native handle leases.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub get_window_properties: unsafe extern "C" fn(*mut Window) -> u32,
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub get_pointer_property: unsafe extern "C" fn(u32, *const c_char, *mut c_void) -> *mut c_void,
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub get_number_property: unsafe extern "C" fn(u32, *const c_char, i64) -> i64,
    pub get_window_surface: unsafe extern "C" fn(*mut Window) -> *mut Surface,
    pub update_window_surface_rects: unsafe extern "C" fn(*mut Window, *const Rect, c_int) -> bool,
    pub convert_pixels: unsafe extern "C" fn(
        c_int,
        c_int,
        u32,
        *const c_void,
        c_int,
        u32,
        *mut c_void,
        c_int,
    ) -> bool,
    pub create_system_cursor: unsafe extern "C" fn(c_int) -> *mut Cursor,
    pub set_cursor: unsafe extern "C" fn(*mut Cursor) -> bool,
    pub destroy_cursor: unsafe extern "C" fn(*mut Cursor),
    pub get_mod_state: unsafe extern "C" fn() -> u16,
    pub start_text_input: unsafe extern "C" fn(*mut Window) -> bool,
    pub start_text_input_with_properties: unsafe extern "C" fn(*mut Window, u32) -> bool,
    pub stop_text_input: unsafe extern "C" fn(*mut Window) -> bool,
    pub set_text_input_area: unsafe extern "C" fn(*mut Window, *const Rect, c_int) -> bool,
    pub create_properties: unsafe extern "C" fn() -> u32,
    pub set_boolean_property: unsafe extern "C" fn(u32, *const c_char, bool) -> bool,
    pub destroy_properties: unsafe extern "C" fn(u32),
    pub set_clipboard_text: unsafe extern "C" fn(*const c_char) -> bool,
    pub get_clipboard_text: unsafe extern "C" fn() -> *mut c_char,
    pub free: unsafe extern "C" fn(*mut c_void),
    pub get_system_theme: unsafe extern "C" fn() -> c_int,
    pub get_desktop_display_mode: unsafe extern "C" fn(u32) -> *const DisplayMode,
    pub get_display_for_window: unsafe extern "C" fn(*mut Window) -> u32,
    pub set_app_metadata_property: unsafe extern "C" fn(*const c_char, *const c_char) -> bool,
    _library: libloading::Library,
}

impl fmt::Debug for Lib {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sdl3::Lib")
    }
}

static LIB: OnceLock<Option<&'static Lib>> = OnceLock::new();

/// The process's SDL3 entry points, or `None` when the library is missing or
/// lacks one of the entry points below. Loading is attempted once.
pub(crate) fn lib() -> Option<&'static Lib> {
    *LIB.get_or_init(|| load().map(|lib| &*Box::leak(Box::new(lib))))
}

/// The library names tried in order.
const LIBRARIES: [&str; 4] = ["libSDL3.so.0", "libSDL3.so", "libSDL3.dylib", "SDL3.dll"];

fn load() -> Option<Lib> {
    // SAFETY: loading a shared library is unsafe only because its initializers
    // run and because the symbol types below are asserted by this module. Every
    // symbol is bound at once, so a partially initialized table is never used.
    unsafe {
        let library = LIBRARIES
            .iter()
            .find_map(|name| libloading::Library::new(name).ok())?;
        let symbol = |name: &[u8]| library.get::<*mut c_void>(name).ok().map(|s| *s);
        macro_rules! bind {
            ($name:literal : $type:ty) => {
                std::mem::transmute::<*mut c_void, $type>(symbol(concat!($name, "\0").as_bytes())?)
            };
        }
        let lib = Lib {
            init: bind!("SDL_Init" : unsafe extern "C" fn(u32) -> bool),
            quit: bind!("SDL_Quit" : unsafe extern "C" fn()),
            get_error: bind!("SDL_GetError" : unsafe extern "C" fn() -> *const c_char),
            get_current_video_driver: bind!("SDL_GetCurrentVideoDriver"
                : unsafe extern "C" fn() -> *const c_char),
            register_events: bind!("SDL_RegisterEvents" : unsafe extern "C" fn(c_int) -> u32),
            poll_event: bind!("SDL_PollEvent" : unsafe extern "C" fn(*mut Event) -> bool),
            wait_event_timeout: bind!("SDL_WaitEventTimeout"
                : unsafe extern "C" fn(*mut Event, c_int) -> bool),
            push_event: bind!("SDL_PushEvent" : unsafe extern "C" fn(*const Event) -> bool),
            create_window: bind!("SDL_CreateWindow"
                : unsafe extern "C" fn(*const c_char, c_int, c_int, u64) -> *mut Window),
            destroy_window: bind!("SDL_DestroyWindow" : unsafe extern "C" fn(*mut Window)),
            show_window: bind!("SDL_ShowWindow" : unsafe extern "C" fn(*mut Window) -> bool),
            set_window_title: bind!("SDL_SetWindowTitle"
                : unsafe extern "C" fn(*mut Window, *const c_char) -> bool),
            get_window_id: bind!("SDL_GetWindowID" : unsafe extern "C" fn(*mut Window) -> u32),
            get_window_flags: bind!("SDL_GetWindowFlags" : unsafe extern "C" fn(*mut Window) -> u64),
            get_window_size: bind!("SDL_GetWindowSize"
                : unsafe extern "C" fn(*mut Window, *mut c_int, *mut c_int) -> bool),
            get_window_size_in_pixels: bind!("SDL_GetWindowSizeInPixels"
                : unsafe extern "C" fn(*mut Window, *mut c_int, *mut c_int) -> bool),
            get_window_display_scale: bind!("SDL_GetWindowDisplayScale"
                : unsafe extern "C" fn(*mut Window) -> f32),
            get_window_properties: bind!("SDL_GetWindowProperties"
                : unsafe extern "C" fn(*mut Window) -> u32),
            get_pointer_property: bind!("SDL_GetPointerProperty"
                : unsafe extern "C" fn(u32, *const c_char, *mut c_void) -> *mut c_void),
            get_number_property: bind!("SDL_GetNumberProperty"
                : unsafe extern "C" fn(u32, *const c_char, i64) -> i64),
            get_window_surface: bind!("SDL_GetWindowSurface"
                : unsafe extern "C" fn(*mut Window) -> *mut Surface),
            update_window_surface_rects: bind!("SDL_UpdateWindowSurfaceRects"
                : unsafe extern "C" fn(*mut Window, *const Rect, c_int) -> bool),
            convert_pixels: bind!("SDL_ConvertPixels" : unsafe extern "C" fn(
                c_int, c_int, u32, *const c_void, c_int, u32, *mut c_void, c_int) -> bool),
            create_system_cursor: bind!("SDL_CreateSystemCursor"
                : unsafe extern "C" fn(c_int) -> *mut Cursor),
            set_cursor: bind!("SDL_SetCursor" : unsafe extern "C" fn(*mut Cursor) -> bool),
            destroy_cursor: bind!("SDL_DestroyCursor" : unsafe extern "C" fn(*mut Cursor)),
            get_mod_state: bind!("SDL_GetModState" : unsafe extern "C" fn() -> u16),
            start_text_input: bind!("SDL_StartTextInput"
                : unsafe extern "C" fn(*mut Window) -> bool),
            start_text_input_with_properties: bind!("SDL_StartTextInputWithProperties"
                : unsafe extern "C" fn(*mut Window, u32) -> bool),
            stop_text_input: bind!("SDL_StopTextInput" : unsafe extern "C" fn(*mut Window) -> bool),
            set_text_input_area: bind!("SDL_SetTextInputArea"
                : unsafe extern "C" fn(*mut Window, *const Rect, c_int) -> bool),
            create_properties: bind!("SDL_CreateProperties" : unsafe extern "C" fn() -> u32),
            set_boolean_property: bind!("SDL_SetBooleanProperty"
                : unsafe extern "C" fn(u32, *const c_char, bool) -> bool),
            destroy_properties: bind!("SDL_DestroyProperties" : unsafe extern "C" fn(u32)),
            set_clipboard_text: bind!("SDL_SetClipboardText"
                : unsafe extern "C" fn(*const c_char) -> bool),
            get_clipboard_text: bind!("SDL_GetClipboardText" : unsafe extern "C" fn() -> *mut c_char),
            free: bind!("SDL_free" : unsafe extern "C" fn(*mut c_void)),
            get_system_theme: bind!("SDL_GetSystemTheme" : unsafe extern "C" fn() -> c_int),
            get_desktop_display_mode: bind!("SDL_GetDesktopDisplayMode"
                : unsafe extern "C" fn(u32) -> *const DisplayMode),
            get_display_for_window: bind!("SDL_GetDisplayForWindow"
                : unsafe extern "C" fn(*mut Window) -> u32),
            set_app_metadata_property: bind!("SDL_SetAppMetadataProperty"
                : unsafe extern "C" fn(*const c_char, *const c_char) -> bool),
            _library: library,
        };
        Some(lib)
    }
}

/// SDL's last error message, or an empty string.
pub(crate) fn error(lib: &Lib) -> String {
    // SAFETY: SDL_GetError returns a NUL-terminated string that lives until the
    // next SDL call on this thread; the copy happens inside this function.
    unsafe {
        let pointer = (lib.get_error)();
        if pointer.is_null() {
            return String::new();
        }
        CStr::from_ptr(pointer).to_string_lossy().into_owned()
    }
}

/// A UTF-8 C string, or `None` when `text` holds a NUL byte.
pub(crate) fn cstring(text: &str) -> Option<CString> {
    CString::new(text).ok()
}

/// Copies a NUL-terminated string SDL lent for the duration of one event.
///
/// # Safety
/// `pointer` must be null or a valid NUL-terminated string for the duration of
/// the call.
pub(crate) unsafe fn string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string.
    Some(
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// A borrowed slice over `pixels`, `length` bytes long.
///
/// # Safety
/// `pixels` must point to `length` writable bytes that stay borrowed for `'a`
/// and hold no other live reference.
pub(crate) unsafe fn bytes<'a>(pixels: *mut c_void, length: usize) -> &'a mut [u8] {
    // SAFETY: the caller guarantees the extent and exclusivity.
    unsafe { std::slice::from_raw_parts_mut(pixels.cast::<u8>(), length) }
}
