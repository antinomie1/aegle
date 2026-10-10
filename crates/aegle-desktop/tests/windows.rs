//! Windows desktop services against the real shell: a global shortcut
//! pressed on the keyboard, file dialogs driven through their controls, the
//! tray icon registered with the notification area, and a notification. It
//! types into the dialogs it opens and presses a global shortcut, so it runs
//! only on a dedicated Windows test desktop:
//! `cargo test -p aegle-desktop --test windows -- --ignored`.
#![cfg(windows)]
#![allow(unsafe_code)]

use aegle_desktop::{Desktop, Event, FileDialog, Icon, MenuItem, Notification, Shortcut, Tray};
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, channel},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Input::KeyboardAndMouse::*,
            Shell::{NIM_MODIFY, NIN_SELECT, NOTIFYICONDATAW, Shell_NotifyIconW},
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

/// The services' tray callback message (`WM_APP + 1`).
const TRAY: u32 = WM_APP + 1;

fn next(events: &Receiver<Event>, within: Duration) -> Option<Event> {
    events.recv_timeout(within).ok()
}

fn key(key: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                ..Default::default()
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    // SAFETY: initialized keyboard INPUT records.
    assert_eq!(
        unsafe { SendInput(inputs, size_of::<INPUT>() as i32) } as usize,
        inputs.len()
    );
}

/// Waits for a dialog titled `title` and brings it to the foreground.
fn dialog(title: &str) -> HWND {
    let title: Vec<u16> = title.encode_utf16().chain([0]).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // SAFETY: a NUL-terminated title; plain window queries.
        unsafe {
            if let Ok(hwnd) = FindWindowW(w!("#32770"), PCWSTR(title.as_ptr()))
                && IsWindowVisible(hwnd).as_bool()
            {
                let _ = SetForegroundWindow(hwnd);
                std::thread::sleep(Duration::from_millis(500));
                return hwnd;
            }
        }
        assert!(Instant::now() < deadline, "no dialog titled {title:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Types `text` into the focused control of `hwnd`, then presses `last`.
fn type_into(hwnd: HWND, text: &str, last: VIRTUAL_KEY) {
    // SAFETY: reads global foreground state only.
    assert_eq!(
        unsafe { GetForegroundWindow() },
        hwnd,
        "the dialog is not in the foreground; refusing to type elsewhere"
    );
    let unicode = |unit: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE
                    | if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                ..Default::default()
            },
        },
    };
    // Select whatever the box holds, so the typed text replaces it.
    send(&[key(VK_CONTROL, false), key(VIRTUAL_KEY(b'A' as u16), false)]);
    send(&[key(VIRTUAL_KEY(b'A' as u16), true), key(VK_CONTROL, true)]);
    for unit in text.encode_utf16() {
        send(&[unicode(unit, false), unicode(unit, true)]);
    }
    std::thread::sleep(Duration::from_millis(200));
    send(&[key(last, false), key(last, true)]);
}

#[test]
#[ignore = "types into dialogs and presses a global shortcut; run on a dedicated Windows test desktop"]
fn shell_services_answer_real_requests() -> std::io::Result<()> {
    let (sink, events) = channel();
    let desktop = Desktop::new("org.aegle.DesktopTest", move |event| {
        let _ = sink.send(event);
    })?;

    // A global shortcut pressed on the keyboard reaches its binding.
    desktop.bind_shortcuts(&[Shortcut {
        id: "test",
        description: "Aegle desktop test",
        trigger: "Ctrl+Alt+Shift+F11".parse().unwrap(),
    }])?;
    assert_eq!(next(&events, Duration::from_millis(300)), None);
    send(&[
        key(VK_CONTROL, false),
        key(VK_MENU, false),
        key(VK_SHIFT, false),
        key(VK_F11, false),
    ]);
    send(&[
        key(VK_F11, true),
        key(VK_SHIFT, true),
        key(VK_MENU, true),
        key(VK_CONTROL, true),
    ]);
    assert_eq!(
        next(&events, Duration::from_secs(2)),
        Some(Event::Shortcut { id: "test".into() })
    );

    // File dialogs: a chosen file, a save name and a cancellation.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let open = desktop.open_file(&FileDialog {
        title: "Aegle open test",
        filters: &[("Manifests", &["*.toml"])],
        ..Default::default()
    })?;
    type_into(
        dialog("Aegle open test"),
        &manifest.to_string_lossy(),
        VK_RETURN,
    );
    assert_eq!(
        next(&events, Duration::from_secs(10)),
        Some(Event::Files {
            request: open,
            paths: Some(vec![manifest.clone()]),
        })
    );
    let target = std::env::temp_dir().join("aegle-desktop-save-test.txt");
    let _ = std::fs::remove_file(&target);
    let save = desktop.save_file(&FileDialog {
        title: "Aegle save test",
        name: "notes.txt",
        ..Default::default()
    })?;
    type_into(
        dialog("Aegle save test"),
        &target.to_string_lossy(),
        VK_RETURN,
    );
    assert_eq!(
        next(&events, Duration::from_secs(10)),
        Some(Event::Files {
            request: save,
            paths: Some(vec![target]),
        })
    );
    let cancel = desktop.open_file(&FileDialog {
        title: "Aegle cancel test",
        ..Default::default()
    })?;
    let hwnd = dialog("Aegle cancel test");
    type_into(hwnd, "", VK_ESCAPE);
    assert_eq!(
        next(&events, Duration::from_secs(10)),
        Some(Event::Files {
            request: cancel,
            paths: None,
        })
    );

    // The notification area accepts the tray icon. Windows 11 keeps a new
    // icon in the hidden overflow, where it has no rectangle to click, so a
    // field-less NIM_MODIFY, which fails for unknown icons, checks it.
    let rgba: Vec<u8> = (0..32 * 32).flat_map(|_| [40, 120, 220, 255]).collect();
    desktop.set_tray(Some(&Tray {
        icon: Icon {
            width: 32,
            height: 32,
            rgba: &rgba,
        },
        tooltip: "Aegle desktop test",
        menu: &[
            MenuItem::Item {
                id: 6,
                label: "Disabled",
                enabled: false,
                checked: None,
            },
            MenuItem::Item {
                id: 7,
                label: "Seven",
                enabled: true,
                checked: Some(true),
            },
        ],
    }))?;
    assert_eq!(next(&events, Duration::from_millis(500)), None);
    // SAFETY: looks up the services' hidden window by its class.
    let owner = unsafe { FindWindowW(w!("Aegle.Desktop.v1"), None) }.unwrap();
    assert!(registered(owner));
    // The shell's version-4 callbacks, posted as the shell posts them: a
    // selection activates the tray, a context menu request shows the real
    // popup menu, chosen here with the keyboard past its disabled item.
    let callback = |event: u32, x: i32, y: i32| {
        let anchor = WPARAM(((y as u32 as usize & 0xffff) << 16) | (x as u32 as usize & 0xffff));
        // SAFETY: posting to a window of another thread of this process.
        unsafe {
            PostMessageW(
                Some(owner),
                TRAY,
                anchor,
                LPARAM((1 << 16) | event as isize),
            )
        }
        .unwrap();
    };
    callback(NIN_SELECT, 400, 400);
    assert_eq!(
        next(&events, Duration::from_secs(2)),
        Some(Event::TrayActivated)
    );
    callback(WM_CONTEXTMENU, 400, 400);
    let deadline = Instant::now() + Duration::from_secs(5);
    // SAFETY: plain window queries.
    while unsafe { FindWindowW(w!("#32768"), None) }.is_err() {
        assert!(Instant::now() < deadline, "the tray menu did not open");
        std::thread::sleep(Duration::from_millis(50));
    }
    // SAFETY: as above; the menu's thread owns the foreground.
    let menu_thread = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
    assert_eq!(menu_thread, unsafe {
        GetWindowThreadProcessId(owner, None)
    });
    // Down reaches "Seven" whether or not the disabled item is skipped.
    for _ in 0..2 {
        send(&[key(VK_DOWN, false), key(VK_DOWN, true)]);
    }
    send(&[key(VK_RETURN, false), key(VK_RETURN, true)]);
    assert_eq!(
        next(&events, Duration::from_secs(2)),
        Some(Event::TrayMenu { item: 7 })
    );

    // A notification shows as a toast on the tray icon and reports its end.
    let shown = Instant::now();
    let id = desktop.notify(&Notification {
        summary: "Aegle desktop test",
        body: "A notification from the Windows acceptance test",
        ..Default::default()
    })?;
    let event = next(&events, Duration::from_secs(60));
    eprintln!("notification after {:?}: {event:?}", shown.elapsed());
    assert_eq!(event, Some(Event::NotificationClosed { notification: id }));

    // Removing the tray removes the icon from the notification area.
    desktop.set_tray(None)?;
    assert_eq!(next(&events, Duration::from_millis(500)), None);
    assert!(!registered(owner));
    drop(desktop);
    Ok(())
}

/// Whether the shell knows the services' tray icon.
fn registered(owner: HWND) -> bool {
    let data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: owner,
        uID: 1,
        ..Default::default()
    };
    // SAFETY: a sized NOTIFYICONDATAW that changes no field.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool()
}
