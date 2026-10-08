//! A drag between two windows of one app through the compositor, driven by
//! a virtual pointer in the private Sway: files, then text, reach the target's
//! drop handler and the source's press is cancelled.
#![cfg(all(feature = "wayland", target_os = "linux"))]

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_pointer::ButtonState,
        wl_registry::{self, WlRegistry},
    },
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

use aegle_app::{App, AppOptions, WindowOptions};
use aegle_ui::{DragData, DropEvent, Point, Result, TextSystem};
use aegle_widgets::{CanvasEvent, Widgets};

/// A wlr virtual pointer on the private compositor's seat; positions are
/// in its 640×480 output.
struct Pointer {
    connection: Connection,
    pointer: ZwlrVirtualPointerV1,
    _queue: EventQueue<Ignore>,
    time: u32,
}

struct Ignore;
impl Dispatch<WlRegistry, GlobalListContents> for Ignore {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(Ignore: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(Ignore: ignore ZwlrVirtualPointerV1);

impl Pointer {
    fn new() -> Self {
        let connection = Connection::connect_to_env().unwrap();
        let (globals, queue) = registry_queue_init::<Ignore>(&connection).unwrap();
        let manager: ZwlrVirtualPointerManagerV1 =
            globals.bind(&queue.handle(), 1..=2, ()).unwrap();
        let pointer = manager.create_virtual_pointer(None, &queue.handle(), ());
        connection.flush().unwrap();
        Self {
            connection,
            pointer,
            _queue: queue,
            time: 0,
        }
    }

    fn at(&mut self, x: u32, y: u32) {
        self.time += 16;
        self.pointer.motion_absolute(self.time, x, y, 640, 480);
        self.pointer.frame();
        self.connection.flush().unwrap();
    }

    fn button(&mut self, pressed: bool) {
        self.time += 16;
        let state = if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };
        self.pointer.button(self.time, 0x110, state);
        self.pointer.frame();
        self.connection.flush().unwrap();
    }
}

#[test]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn drags_cross_windows_through_the_compositor() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let app = App::with_fonts(TextSystem::new(), AppOptions::default())?;
    let options = WindowOptions {
        width: 320,
        height: 480,
        ..Default::default()
    };
    let source = app.window_with_options("Source", options)?;
    let target = app.window_with_options("Target", options)?;

    let payloads = Rc::new(RefCell::new(vec![
        DragData::Text("dragged text".into()),
        DragData::Files(vec![PathBuf::from("/tmp/a b/ü.txt")]),
    ]));
    let cancelled = Rc::new(RefCell::new(0));
    let canvas = source.canvas(|_, _| Ok(()))?;
    canvas.set_grow(1.0)?;
    let (next, cancels) = (payloads.clone(), cancelled.clone());
    canvas.on_input(move |canvas, event| match event {
        CanvasEvent::Move { pressed: true, .. } => match next.borrow_mut().pop() {
            Some(data) => canvas.start_drag(data),
            None => Ok(()),
        },
        CanvasEvent::Cancel => {
            *cancels.borrow_mut() += 1;
            Ok(())
        }
        _ => Ok(()),
    })?;
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    target.on_drop(move |_, event| {
        log.borrow_mut().push(event);
        Ok(())
    })?;

    let mut pointer = Pointer::new();
    let pump = |until: &dyn Fn() -> bool| -> Result {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !until() && Instant::now() < deadline {
            app.dispatch(Some(Duration::from_millis(10)))?;
        }
        Ok(())
    };
    let wait = |ms| {
        let end = Instant::now() + Duration::from_millis(ms);
        pump(&|| Instant::now() > end)
    };
    wait(1000)?;
    for drops in 1..=2 {
        // Press in the left window, move into the right one and release.
        pointer.at(160, 240);
        wait(50)?;
        pointer.button(true);
        wait(50)?;
        for x in [180, 200, 260, 380, 420] {
            pointer.at(x, 250);
            wait(50)?;
        }
        pointer.button(false);
        pump(&|| {
            seen.borrow()
                .iter()
                .filter(|e| matches!(e, DropEvent::Drop { .. }))
                .count()
                == drops
        })?;
    }
    let seen = seen.borrow();
    let dropped: Vec<_> = seen
        .iter()
        .filter_map(|event| match event {
            DropEvent::Drop { data, position } => Some((data.clone(), *position)),
            _ => None,
        })
        .collect();
    assert_eq!(seen[0], DropEvent::Enter);
    assert_eq!(dropped.len(), 2, "{seen:?}");
    assert_eq!(
        dropped[0].0,
        DragData::Files(vec![PathBuf::from("/tmp/a b/ü.txt")])
    );
    assert_eq!(dropped[1].0, DragData::Text("dragged text".into()));
    // The target window starts at x = 320 in the 640 px output.
    assert_eq!(dropped[0].1, Point::new(100.0, 250.0));
    assert_eq!(*cancelled.borrow(), 2);
    Ok(())
}
