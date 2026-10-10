//! OLE drag and drop between two windows of one app with the real mouse:
//! files, then text, reach the target's drop handler and the source's press
//! is cancelled. It moves the system pointer, so it runs only on a dedicated
//! Windows test desktop: `AEGLE_TEST_COMPOSITOR=private cargo test -p
//! aegle-app --features windows,software --test drag_windows -- --ignored`.
#![cfg(all(feature = "windows", feature = "software", target_os = "windows"))]
#![allow(unsafe_code)]

use aegle_app::{App, AppOptions, WindowOptions};
use aegle_ui::{DragData, DropEvent, Point, Result, TextSystem};
use aegle_widgets::{CanvasEvent, Widgets};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ClientToScreen,
        UI::{HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::PCWSTR,
};

/// A top-level window of this process by title.
fn window(title: &str) -> HWND {
    let title: Vec<u16> = title.encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated title.
    unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap()
}

/// Sends one absolute mouse event at a physical screen point.
fn mouse(x: i32, y: i32, flags: MOUSE_EVENT_FLAGS) {
    // SAFETY: plain metrics queries and one initialized INPUT record.
    unsafe {
        let (left, top) = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        );
        let (width, height) = (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: (x - left) * 65535 / (width - 1),
                    dy: (y - top) * 65535 / (height - 1),
                    dwFlags: flags
                        | MOUSEEVENTF_MOVE
                        | MOUSEEVENTF_ABSOLUTE
                        | MOUSEEVENTF_VIRTUALDESK,
                    ..Default::default()
                },
            },
        };
        assert_eq!(SendInput(&[input], size_of::<INPUT>() as i32), 1);
    }
}

/// The physical screen origin of a window's client area.
fn origin(hwnd: HWND) -> POINT {
    let mut origin = POINT::default();
    // SAFETY: a live window of this process.
    let _ = unsafe { ClientToScreen(hwnd, &mut origin) };
    origin
}

#[test]
#[ignore = "moves the system pointer; run on a dedicated Windows test desktop"]
fn ole_drag_carries_files_and_text_between_windows() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let app = App::with_fonts(TextSystem::new(), AppOptions::default())?;
    let options = WindowOptions {
        width: 320,
        height: 240,
        ..Default::default()
    };
    let source = app.window_with_options("Aegle drag source", options)?;
    let target = app.window_with_options("Aegle drag target", options)?;

    let file = PathBuf::from(r"C:\a b\ü.txt");
    let payloads = Rc::new(RefCell::new(vec![
        DragData::Text("dragged text".into()),
        DragData::Files(vec![file.clone()]),
    ]));
    let cancelled = Rc::new(RefCell::new(0));
    let canvas = source.canvas(|_, _| {});
    canvas.set_grow(1.0);
    let (next, cancels) = (payloads.clone(), cancelled.clone());
    canvas.set_input(move |canvas, event| match event {
        CanvasEvent::Move { pressed: true, .. } => {
            if let Some(data) = next.borrow_mut().pop() {
                canvas.start_drag(data);
            }
        }
        CanvasEvent::Cancel => *cancels.borrow_mut() += 1,
        _ => {}
    });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    target.on_drop(move |_, event| {
        log.borrow_mut().push(event);
        Ok(())
    });
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
    wait(500)?;

    // Side by side and above other windows, so the drag crosses only them.
    let (from, to) = (window("Aegle drag source"), window("Aegle drag target"));
    // SAFETY: windows of this thread; only their placement changes.
    unsafe {
        for (hwnd, x) in [(from, 100), (to, 600)] {
            SetWindowPos(hwnd, Some(HWND_TOPMOST), x, 100, 0, 0, SWP_NOSIZE).unwrap();
        }
    }
    wait(300)?;
    let (a, b) = (origin(from), origin(to));
    // SAFETY: a live window of this process.
    let scale = unsafe { GetDpiForWindow(to) } as f32 / 96.0;
    let local = Point::new(100.0, 120.0);
    let end = (
        b.x + (local.x * scale) as i32,
        b.y + (local.y * scale) as i32,
    );

    for drops in 1..=2 {
        // The source blocks in SHDoDragDrop until the drop, so the pointer
        // moves from another thread while this one keeps dispatching.
        let press = (a.x + (160.0 * scale) as i32, a.y + (120.0 * scale) as i32);
        let pointer = std::thread::spawn(move || {
            let pause = || std::thread::sleep(Duration::from_millis(60));
            mouse(press.0, press.1, MOUSE_EVENT_FLAGS(0));
            pause();
            mouse(press.0, press.1, MOUSEEVENTF_LEFTDOWN);
            pause();
            for step in 1..=10 {
                let x = press.0 + (end.0 - press.0) * step / 10;
                let y = press.1 + (end.1 - press.1) * step / 10;
                mouse(x, y, MOUSE_EVENT_FLAGS(0));
                pause();
            }
            mouse(end.0, end.1, MOUSEEVENTF_LEFTUP);
        });
        pump(&|| {
            seen.borrow()
                .iter()
                .filter(|e| matches!(e, DropEvent::Drop { .. }))
                .count()
                == drops
        })?;
        pointer.join().unwrap();
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
    assert_eq!(dropped[0].0, DragData::Files(vec![file]));
    assert_eq!(dropped[1].0, DragData::Text("dragged text".into()));
    for (_, position) in &dropped {
        assert!(
            (position.x - local.x).abs() <= 1.0 && (position.y - local.y).abs() <= 1.0,
            "{position:?}"
        );
    }
    assert_eq!(*cancelled.borrow(), 2);
    Ok(())
}
