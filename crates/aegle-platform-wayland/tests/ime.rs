//! Native protocol regression; run only on an explicitly selected private compositor.

use aegle_platform_wayland::{
    Event, ImeCause, ImeEvent, ImeRequest, ImeUpdate, Wayland, WindowId, WindowOptions,
};
use std::{
    fs::OpenOptions,
    io::Write,
    os::fd::AsFd,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat::WlSeat},
};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{
        zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        zwp_input_method_v2::{self, ZwpInputMethodV2},
    },
    zwp_virtual_keyboard_v1::client::{
        zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
        zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
    },
};
use xkbcommon::xkb;

#[derive(Default)]
struct InputMethod {
    serial: u32,
    active: bool,
    surrounding: String,
    activations: u32,
    surrounding_events: u32,
}
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for InputMethod {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwpInputMethodV2, ()> for InputMethod {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => {
                state.active = true;
                state.activations += 1;
            }
            zwp_input_method_v2::Event::Deactivate => state.active = false,
            zwp_input_method_v2::Event::SurroundingText { text, .. } => {
                state.surrounding = text;
                state.surrounding_events += 1;
            }
            zwp_input_method_v2::Event::Done => state.serial = state.serial.wrapping_add(1),
            zwp_input_method_v2::Event::Unavailable => {
                panic!("private seat already has an input method")
            }
            _ => {}
        }
    }
}
delegate_noop!(InputMethod: ignore WlSeat);
delegate_noop!(InputMethod: ignore ZwpInputMethodManagerV2);
delegate_noop!(InputMethod: ignore ZwpVirtualKeyboardManagerV1);
delegate_noop!(InputMethod: ignore ZwpVirtualKeyboardV1);

struct Probe {
    app: Wayland,
    queue: EventQueue<InputMethod>,
    method: ZwpInputMethodV2,
    state: InputMethod,
    events: Vec<(WindowId, ImeEvent)>,
}
impl Probe {
    fn pump(&mut self, ready: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.app.dispatch(Some(Duration::from_millis(10))).unwrap();
            while let Some(event) = self.app.next_event() {
                match event {
                    Event::Redraw { window } => {
                        self.app
                            .present::<()>(window, |pixels, _| {
                                pixels.fill(255);
                                Ok(())
                            })
                            .unwrap();
                    }
                    Event::Ime { window, event, .. } => self.events.push((window, event)),
                    Event::Error(error) => panic!("native input failure: {error}"),
                    _ => {}
                }
            }
            self.queue.roundtrip(&mut self.state).unwrap();
            if ready(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "native IME timeout: {:?}",
                self.events
            );
        }
    }
    fn update(&mut self) -> ImeUpdate {
        self.pump(|p| {
            p.events
                .iter()
                .any(|(_, event)| matches!(event, ImeEvent::Update(_)))
        });
        self.events
            .drain(..)
            .find_map(|(_, event)| {
                if let ImeEvent::Update(update) = event {
                    Some(update)
                } else {
                    None
                }
            })
            .unwrap()
    }
    fn commit(&mut self, text: &str) {
        self.method.commit_string(text.into());
        self.method.commit(self.state.serial);
    }
}

#[test]
#[ignore = "requires a private compositor with input-method-v2 and virtual-keyboard-v1"]
fn native_ime_batches_and_session_boundaries() {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private"),
        "explicitly select a private compositor before running this test"
    );
    let connection = Connection::connect_to_env().unwrap();
    let (globals, mut queue) = registry_queue_init::<InputMethod>(&connection).unwrap();
    let handle = queue.handle();
    let seat: WlSeat = globals.bind(&handle, 1..=7, ()).unwrap();
    let manager: ZwpInputMethodManagerV2 = globals.bind(&handle, 1..=1, ()).unwrap();
    let method = manager.get_input_method(&seat, &handle, ());
    let keys: ZwpVirtualKeyboardManagerV1 = globals.bind(&handle, 1..=1, ()).unwrap();
    let keyboard = keys.create_virtual_keyboard(&seat, &handle, ());
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(
        &context,
        "",
        "",
        "us",
        "",
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .unwrap();
    let text = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1) + "\0";
    let path = std::env::temp_dir().join(format!("aegle-ime-keymap-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::remove_file(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
    keyboard.keymap(1, file.as_fd(), text.len() as u32);
    let mut state = InputMethod::default();
    queue.roundtrip(&mut state).unwrap();
    let app = Wayland::connect().unwrap();
    let mut p = Probe {
        app,
        queue,
        method,
        state,
        events: Vec::new(),
    };
    let first = p.app.create_window(WindowOptions::default()).unwrap();
    let mut req = ImeRequest {
        surrounding: Some("你好 world".into()),
        cursor: 6,
        anchor: 6,
        ..Default::default()
    };
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.pump(|p| p.state.active && p.state.surrounding == "你好 world");
    p.events.clear();
    p.method.set_preedit_string("中文".into(), 3, 6);
    p.method.commit(p.state.serial);
    let preedit = p.update();
    assert_eq!(
        (preedit.preedit.text.as_str(), preedit.preedit.cursor),
        ("中文", Some((3, 6)))
    );
    assert!(preedit.current);
    p.method.delete_surrounding_text(3, 1);
    p.method.set_preedit_string("续".into(), -1, -1);
    p.commit("界");
    let batch = p.update();
    assert_eq!(batch.commit.as_deref(), Some("界"));
    assert_eq!((batch.delete_before, batch.delete_after), (3, 1));
    assert_eq!(
        (batch.preedit.text.as_str(), batch.preedit.cursor),
        ("续", None)
    );

    // Queue an old-server-state transaction before flushing the new client commit.
    req.cursor = 3;
    req.anchor = 3;
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.commit("古");
    p.queue.roundtrip(&mut p.state).unwrap();
    let stale = p.update();
    assert!(!stale.current);
    assert_eq!(stale.commit.as_deref(), Some("古"));
    assert!(stale.preedit.text.is_empty());
    assert_eq!((stale.delete_before, stale.delete_after), (0, 0));
    let serial = p.state.serial;
    req.surrounding = Some("你古好 world".into());
    req.cursor = 6;
    req.anchor = 6;
    req.cause = ImeCause::InputMethod;
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.pump(|_| true);
    assert_eq!(
        p.state.serial, serial,
        "stale transactions defer state publication"
    );
    p.commit("今");
    assert!(p.update().current);
    req.surrounding = Some("你古今好 world".into());
    req.cursor = 9;
    req.anchor = 9;
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.pump(|p| Some(&p.state.surrounding) == req.surrounding.as_ref());

    // A selection too large for surrounding text still supports composition.
    let activations = p.state.activations;
    let surroundings = p.state.surrounding_events;
    p.app
        .configure_ime(
            first,
            Some(ImeRequest {
                surrounding: None,
                ..Default::default()
            }),
        )
        .unwrap();
    p.pump(|p| p.state.activations > activations);
    assert!(p.state.active);
    assert_eq!(p.state.surrounding_events, surroundings);
    p.events.clear();
    p.method.set_preedit_string("无".into(), 3, 3);
    p.method.commit(p.state.serial);
    assert_eq!(p.update().preedit.text, "无");
    let activations = p.state.activations;
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.pump(|p| p.state.activations > activations);
    assert_eq!(Some(&p.state.surrounding), req.surrounding.as_ref());

    let second = p.app.create_window(WindowOptions::default()).unwrap();
    p.pump(|p| p.events.contains(&(first, ImeEvent::Left)));
    assert!(!p.state.active);
    p.events.clear();
    p.app.remove_window(second).unwrap();
    p.pump(|p| p.state.active && p.events.contains(&(first, ImeEvent::Entered)));
    assert_eq!(Some(&p.state.surrounding), req.surrounding.as_ref());

    // Cancellation also discards edits already dispatched into the app queue.
    p.events.clear();
    p.commit("queued before cancel");
    p.queue.roundtrip(&mut p.state).unwrap();
    p.app.dispatch(Some(Duration::from_millis(100))).unwrap();
    p.app.configure_ime(first, None).unwrap();
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.pump(|p| p.state.active);
    assert!(
        !p.events
            .iter()
            .any(|(_, event)| matches!(event, ImeEvent::Update(_)))
    );

    // A queued old-editor transaction must not enter the newly enabled editor.
    p.events.clear();
    p.app.configure_ime(first, None).unwrap();
    p.commit("old session");
    p.queue.roundtrip(&mut p.state).unwrap();
    p.app.configure_ime(first, Some(req.clone())).unwrap();
    p.pump(|p| p.state.active);
    assert!(
        !p.events
            .iter()
            .any(|(_, event)| matches!(event, ImeEvent::Update(_)))
    );
    // A delayed done after disabling must not block the next enable indefinitely.
    p.app.configure_ime(first, None).unwrap();
    p.commit("late");
    p.queue.roundtrip(&mut p.state).unwrap();
    p.pump(|p| !p.state.active);
    p.app.configure_ime(first, Some(req)).unwrap();
    p.pump(|p| p.state.active);
    p.app.remove_window(first).unwrap();
}
