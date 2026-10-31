//! Native input translation, driven through SDL's own event injection.
//!
//! SDL3 queues events pushed with `SDL_PushEvent` exactly like ones a driver
//! produced, so a test can hand the backend the same structures the platform
//! delivers: keys, produced text, pointer, wheel, fingers and drops. The
//! structures below repeat the C layout from `SDL_events.h`, which is the ABI
//! the backend reads.
//!
//! The dummy video driver keeps this runnable without a display; because SDL's
//! video state is process-wide, the tests below take turns on one mutex.
#![allow(unsafe_code)]

use std::{
    collections::VecDeque,
    ffi::{c_char, c_int, c_void},
    path::PathBuf,
    sync::{Mutex, MutexGuard, Once},
    time::Duration,
};

use aegle_platform::{
    DragData, Event, Key, Modifiers, PixelSize, Platform, PointerButton, PointerKind, SeatId,
    TouchPhase, WindowOptions,
};
use aegle_platform_sdl3::Sdl3;

// --- the SDL3 surface this test drives ------------------------------------

mod sdl {
    use super::{c_char, c_int, c_void};

    pub const KEY_DOWN: u32 = 0x300;
    pub const TEXT_INPUT: u32 = 0x303;
    pub const MOUSE_MOTION: u32 = 0x400;
    pub const MOUSE_BUTTON_DOWN: u32 = 0x401;
    pub const MOUSE_WHEEL: u32 = 0x403;
    pub const FINGER_DOWN: u32 = 0x700;
    pub const DROP_FILE: u32 = 0x1000;
    pub const DROP_BEGIN: u32 = 0x1002;
    pub const DROP_COMPLETE: u32 = 0x1003;
    pub const DROP_POSITION: u32 = 0x1004;
    pub const TOUCH_MOUSEID: u32 = u32::MAX;

    #[repr(C)]
    pub struct Window {
        _private: [u8; 0],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct WindowEvent {
        pub kind: u32,
        pub reserved: u32,
        pub timestamp: u64,
        pub window_id: u32,
        pub data1: i32,
        pub data2: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct KeyboardEvent {
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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct TextInputEvent {
        pub kind: u32,
        pub reserved: u32,
        pub timestamp: u64,
        pub window_id: u32,
        pub text: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct MouseMotionEvent {
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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct MouseButtonEvent {
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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct MouseWheelEvent {
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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct TouchFingerEvent {
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

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct DropEvent {
        pub kind: u32,
        pub reserved: u32,
        pub timestamp: u64,
        pub window_id: u32,
        pub x: f32,
        pub y: f32,
        pub source: *const c_char,
        pub data: *const c_char,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union Event {
        pub kind: u32,
        pub window: WindowEvent,
        pub key: KeyboardEvent,
        pub text: TextInputEvent,
        pub motion: MouseMotionEvent,
        pub button: MouseButtonEvent,
        pub wheel: MouseWheelEvent,
        pub finger: TouchFingerEvent,
        pub drop: DropEvent,
        pub padding: [u8; 128],
    }

    impl Event {
        pub fn of(kind: u32) -> Self {
            // SAFETY: every field is plain data and all-zero is valid.
            let mut event: Self = unsafe { std::mem::zeroed() };
            event.kind = kind;
            event
        }
    }

    /// The process's SDL3 entry points.
    pub struct Api {
        pub push_event: unsafe extern "C" fn(*const Event) -> bool,
        pub get_keyboard_focus: unsafe extern "C" fn() -> *mut Window,
        pub get_window_id: unsafe extern "C" fn(*mut Window) -> u32,
        _library: libloading::Library,
    }

    impl Api {
        pub fn load() -> Self {
            // SAFETY: the library is already loaded by the backend; the symbol
            // types below are asserted here.
            unsafe {
                let library = libloading::Library::new("libSDL3.so.0").unwrap();
                let bind = |name: &[u8]| *library.get::<*mut c_void>(name).unwrap();
                Self {
                    push_event: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*const Event) -> bool,
                    >(bind(b"SDL_PushEvent\0")),
                    get_keyboard_focus: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn() -> *mut Window,
                    >(bind(b"SDL_GetKeyboardFocus\0")),
                    get_window_id: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*mut Window) -> u32,
                    >(bind(b"SDL_GetWindowID\0")),
                    _library: library,
                }
            }
        }

        pub fn push(&self, event: &Event) {
            // SAFETY: SDL copies the event; it holds no pointers but the
            // NUL-terminated text kept alive by the caller.
            assert!(unsafe { (self.push_event)(event) });
        }

        /// The SDL window ID of the focused window.
        pub fn focused_window(&self) -> u32 {
            // SAFETY: a query; SDL returns null when no window has focus.
            unsafe {
                let window = (self.get_keyboard_focus)();
                assert!(!window.is_null(), "the test window holds focus");
                (self.get_window_id)(window)
            }
        }
    }
}

// --- the test's own driver-independent scaffolding ------------------------

static SERIAL: Mutex<()> = Mutex::new(());

fn platform() -> (MutexGuard<'static, ()>, Sdl3, sdl::Api) {
    let guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    static DRIVER: Once = Once::new();
    DRIVER.call_once(|| {
        // SAFETY: no other thread of this binary touches the environment before
        // the serialized first connection.
        unsafe { std::env::set_var("SDL_VIDEODRIVER", "dummy") };
    });
    let mut platform = Sdl3::connect().unwrap();
    platform.dispatch(Some(Duration::ZERO)).unwrap();
    let window = platform
        .create_window(WindowOptions {
            title: "input",
            app_id: "org.aegle.test",
            size: PixelSize {
                width: 200,
                height: 100,
            },
            buffer_budget: usize::MAX,
            layer: None,
        })
        .unwrap();
    // The dummy driver focuses the first window; its configure, first redraw
    // and focus report are dropped so each scenario sees only its own events.
    platform.dispatch(Some(Duration::from_millis(10))).unwrap();
    while platform.next_event().is_some() {}
    let api = sdl::Api::load();
    let _ = window;
    (guard, platform, api)
}

/// Pushes `events`, dispatches once, and returns every contract event.
fn push(platform: &mut Sdl3, api: &sdl::Api, events: &[sdl::Event]) -> VecDeque<Event> {
    for event in events {
        api.push(event);
    }
    platform.dispatch(Some(Duration::ZERO)).unwrap();
    std::iter::from_fn(|| platform.next_event()).collect()
}

fn only(events: VecDeque<Event>) -> Event {
    let mut events: Vec<_> = events.into_iter().collect();
    let last = events.pop().expect("one contract event");
    assert!(events.is_empty(), "unexpected extra events: {events:?}");
    last
}

// --- the scenarios --------------------------------------------------------

#[test]
fn keys_report_identity_modifiers_and_no_text() {
    let (_guard, mut platform, api) = platform();
    let window = api.focused_window();
    let mut event = sdl::Event::of(sdl::KEY_DOWN);
    event.key = sdl::KeyboardEvent {
        kind: sdl::KEY_DOWN,
        window_id: window,
        key: 0x4000_0050,  // SDLK_LEFT
        modifiers: 0x0003, // Shift
        down: 1,
        repeat: 0,
        ..Default::default()
    };
    let event = only(push(&mut platform, &api, &[event]));
    let Event::Key {
        key,
        text,
        pressed,
        repeat,
        modifiers,
        seat,
        ..
    } = event
    else {
        panic!("not a key event: {event:?}")
    };
    assert_eq!(key, Key::Left);
    assert_eq!(text, None, "SDL reports text separately");
    assert!(pressed);
    assert!(!repeat);
    assert_eq!(
        modifiers,
        Modifiers {
            shift: true,
            ..Default::default()
        }
    );
    assert_eq!(seat, SeatId(0));
}

#[test]
fn produced_text_arrives_as_its_own_event() {
    let (_guard, mut platform, api) = platform();
    let window = api.focused_window();
    let text = std::ffi::CString::new("é世").unwrap();
    let mut event = sdl::Event::of(sdl::TEXT_INPUT);
    event.text = sdl::TextInputEvent {
        kind: sdl::TEXT_INPUT,
        window_id: window,
        text: text.as_ptr(),
        ..Default::default()
    };
    let event = only(push(&mut platform, &api, &[event]));
    let Event::Text { text: produced, .. } = event else {
        panic!("not a text event: {event:?}")
    };
    assert_eq!(produced, "é世");
}

#[test]
fn pointer_motion_buttons_and_wheel_arrive_in_logical_pixels() {
    let (_guard, mut platform, api) = platform();
    let window = api.focused_window();
    let mut motion = sdl::Event::of(sdl::MOUSE_MOTION);
    motion.motion = sdl::MouseMotionEvent {
        kind: sdl::MOUSE_MOTION,
        window_id: window,
        x: 10.0,
        y: 20.0,
        ..Default::default()
    };
    let mut down = sdl::Event::of(sdl::MOUSE_BUTTON_DOWN);
    down.button = sdl::MouseButtonEvent {
        kind: sdl::MOUSE_BUTTON_DOWN,
        window_id: window,
        button: 1,
        down: 1,
        clicks: 2,
        x: 10.0,
        y: 20.0,
        ..Default::default()
    };
    let mut secondary = sdl::Event::of(sdl::MOUSE_BUTTON_DOWN);
    secondary.button = sdl::MouseButtonEvent {
        kind: sdl::MOUSE_BUTTON_DOWN,
        window_id: window,
        button: 3,
        down: 1,
        clicks: 1,
        x: 10.0,
        y: 20.0,
        ..Default::default()
    };
    // SDL's vertical wheel amount is positive away from the user.
    let mut wheel = sdl::Event::of(sdl::MOUSE_WHEEL);
    wheel.wheel = sdl::MouseWheelEvent {
        kind: sdl::MOUSE_WHEEL,
        window_id: window,
        y: -1.0,
        mouse_x: 10.0,
        mouse_y: 20.0,
        ..Default::default()
    };
    // A finger's synthesized mouse event must not become a second pointer.
    let mut synthetic = sdl::Event::of(sdl::MOUSE_MOTION);
    synthetic.motion = sdl::MouseMotionEvent {
        kind: sdl::MOUSE_MOTION,
        window_id: window,
        which: sdl::TOUCH_MOUSEID,
        x: 30.0,
        y: 40.0,
        ..Default::default()
    };
    let events = push(
        &mut platform,
        &api,
        &[motion, down, secondary, wheel, synthetic],
    );
    let kinds: Vec<_> = events
        .into_iter()
        .map(|event| {
            let Event::Pointer { position, kind, .. } = event else {
                panic!("not a pointer event: {event:?}")
            };
            (position, kind)
        })
        .collect();
    assert_eq!(kinds.len(), 4, "the synthesized motion was dropped");
    assert_eq!(kinds[0].0, aegle_platform::Point::new(10.0, 20.0));
    assert_eq!(kinds[0].1, PointerKind::Move);
    assert_eq!(kinds[1].1, PointerKind::Down { clicks: 2 });
    assert_eq!(
        kinds[2].1,
        PointerKind::ButtonDown(PointerButton::Secondary)
    );
    let PointerKind::Scroll { delta } = kinds[3].1 else {
        panic!("not a scroll: {:?}", kinds[3].1)
    };
    assert!(delta.y > 0.0, "downward scrolling is positive");
    assert_eq!(delta.x, 0.0);
}

#[test]
fn fingers_arrive_in_logical_coordinates() {
    let (_guard, mut platform, api) = platform();
    let window = api.focused_window();
    let mut finger = sdl::Event::of(sdl::FINGER_DOWN);
    finger.finger = sdl::TouchFingerEvent {
        kind: sdl::FINGER_DOWN,
        window_id: window,
        finger_id: 7,
        x: 0.5,
        y: 0.25,
        ..Default::default()
    };
    let event = only(push(&mut platform, &api, &[finger]));
    let Event::Touch {
        finger,
        position,
        phase,
        ..
    } = event
    else {
        panic!("not a touch event: {event:?}")
    };
    assert_eq!(finger, 7);
    assert_eq!(phase, TouchPhase::Down);
    // SDL normalizes fingers; the contract uses window logical coordinates.
    assert_eq!(position, aegle_platform::Point::new(100.0, 25.0));
}

#[test]
fn a_dropped_drag_carries_files_and_a_refused_one_leaves() {
    let (_guard, mut platform, api) = platform();
    let window = api.focused_window();
    let contract_window = aegle_platform::WindowId(0);
    let first = std::ffi::CString::new("/tmp/aegle one.txt").unwrap();
    let second = std::ffi::CString::new("/tmp/aegle-two.txt").unwrap();
    let drop_event = |kind: u32, x: f32, y: f32, data: *const c_char| {
        let mut event = sdl::Event::of(kind);
        event.drop = sdl::DropEvent {
            kind,
            window_id: window,
            x,
            y,
            data,
            ..Default::default()
        };
        event
    };
    let sequence = || {
        [
            drop_event(sdl::DROP_BEGIN, 0.0, 0.0, std::ptr::null()),
            drop_event(sdl::DROP_POSITION, 5.0, 6.0, std::ptr::null()),
            drop_event(sdl::DROP_FILE, 5.0, 6.0, first.as_ptr()),
            drop_event(sdl::DROP_FILE, 6.0, 7.0, second.as_ptr()),
            drop_event(sdl::DROP_COMPLETE, 6.0, 7.0, std::ptr::null()),
        ]
    };

    let events = push(&mut platform, &api, &sequence());
    let mut events: Vec<_> = events
        .into_iter()
        .filter(|event| !matches!(event, Event::Configure { .. }))
        .collect();
    assert_eq!(events.len(), 2, "{events:?}");
    let drop = events.pop().unwrap();
    let Event::Drop {
        position,
        data,
        window: dropped,
        ..
    } = drop
    else {
        panic!("not a drop: {drop:?}")
    };
    assert_eq!(dropped, contract_window);
    assert_eq!(position, aegle_platform::Point::new(6.0, 7.0));
    assert_eq!(
        data,
        DragData::Files(vec![
            PathBuf::from("/tmp/aegle one.txt"),
            PathBuf::from("/tmp/aegle-two.txt")
        ])
    );

    // A target that refuses the drag gets a leave instead of the data. The
    // begin event resets the previous answer, so the refusal comes with the
    // drag's first position, as a host answering `Event::Drag` would send it.
    let sequence = sequence();
    push(&mut platform, &api, &sequence[..2]);
    platform
        .accept_drag(contract_window, SeatId(0), false)
        .unwrap();
    let events = push(&mut platform, &api, &sequence[2..]);
    let events: Vec<_> = events
        .into_iter()
        .filter(|event| !matches!(event, Event::Configure { .. }))
        .collect();
    assert!(
        matches!(events.last(), Some(Event::DragLeave { .. })),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Drop { .. }))
    );
}
