//! Every Linux service against mock implementations on a private session bus:
//! `AEGLE_TEST_BUS=private dbus-run-session -- cargo test -p aegle-desktop --test linux -- --ignored`.
//! The mocks follow the portal, notification and StatusNotifierItem
//! specifications; the tray mock acts as a tray host would.
#![cfg(target_os = "linux")]

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

use aegle_dbus::{Connection, Kind, Message, Value};
use aegle_desktop::{Desktop, Event, FileDialog, Icon, MenuItem, Notification, Shortcut, Tray};

const SESSION: &str = "/org/freedesktop/portal/desktop/session/test/s";

fn own(bus: &Connection, incoming: &mut aegle_dbus::Incoming, name: &str) {
    let call = Message::call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        vec![Value::str(name), Value::U32(4)],
    );
    let serial = bus.send(&call).unwrap();
    while incoming.recv().unwrap().reply_serial != serial {}
}

/// Answers a portal request: returns its handle, then emits its Response.
fn respond(bus: &Connection, call: &Message, response: u32, results: Value) {
    let options = call.body.last().unwrap();
    let token = options.get("handle_token").and_then(Value::as_str).unwrap();
    let sender = call.sender.trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    bus.send(&call.reply(vec![Value::Path(path.clone())]))
        .unwrap();
    let body = vec![Value::U32(response), results];
    let signal = Message::signal(&path, "org.freedesktop.portal.Request", "Response", body);
    bus.send(&signal).unwrap();
}

/// The mock desktop; it reports what clients sent it on `seen`.
fn desktop(bus: Connection, seen: Sender<Message>) {
    let mut incoming = bus.incoming().unwrap();
    for name in [
        "org.freedesktop.Notifications",
        "org.freedesktop.portal.Desktop",
        "org.kde.StatusNotifierWatcher",
    ] {
        own(&bus, &mut incoming, name);
    }
    let mut item = String::new();
    loop {
        let Ok(message) = incoming.recv() else { return };
        if message.kind == Kind::Return && !item.is_empty() {
            let _ = seen.send(message);
            continue;
        }
        if message.kind != Kind::Call {
            continue;
        }
        let _ = seen.send(message.clone());
        match message.member.as_str() {
            "Notify" => {
                bus.send(&message.reply(vec![Value::U32(7)])).unwrap();
                let path = "/org/freedesktop/Notifications";
                let interface = "org.freedesktop.Notifications";
                let action = vec![Value::U32(7), Value::str("open")];
                bus.send(&Message::signal(path, interface, "ActionInvoked", action))
                    .unwrap();
                let closed = vec![Value::U32(7), Value::U32(2)];
                bus.send(&Message::signal(
                    path,
                    interface,
                    "NotificationClosed",
                    closed,
                ))
                .unwrap();
            }
            "OpenFile" => {
                let uris = Value::strings([
                    "file:///tmp/a%20b.txt",
                    "https://example.org/x",
                    "file:///tmp/c",
                ]);
                respond(&bus, &message, 0, Value::dict([("uris", uris)]));
            }
            "SaveFile" => respond(&bus, &message, 1, Value::dict([])),
            "CreateSession" => {
                let results = Value::dict([("session_handle", Value::str(SESSION))]);
                respond(&bus, &message, 0, results);
            }
            "BindShortcuts" => {
                respond(&bus, &message, 0, Value::dict([]));
                let body = vec![
                    Value::Path(SESSION.into()),
                    Value::str("toggle"),
                    Value::U64(1),
                    Value::dict([]),
                ];
                let interface = "org.freedesktop.portal.GlobalShortcuts";
                let signal = Message::signal(
                    "/org/freedesktop/portal/desktop",
                    interface,
                    "Activated",
                    body,
                );
                bus.send(&signal).unwrap();
            }
            "RegisterStatusNotifierItem" => {
                bus.send(&message.reply(vec![])).unwrap();
                item = message.body[0].as_str().unwrap().into();
                // As a tray host: read the item and its menu, choose an
                // entry and activate the icon.
                let properties = "org.freedesktop.DBus.Properties";
                let menu = "com.canonical.dbusmenu";
                for call in [
                    Message::call(
                        &item,
                        "/StatusNotifierItem",
                        properties,
                        "GetAll",
                        vec![Value::str("org.kde.StatusNotifierItem")],
                    ),
                    Message::call(
                        &item,
                        "/MenuBar",
                        menu,
                        "GetLayout",
                        vec![Value::I32(0), Value::I32(-1), Value::strings([])],
                    ),
                    Message::call(
                        &item,
                        "/MenuBar",
                        menu,
                        "Event",
                        vec![
                            Value::I32(4),
                            Value::str("clicked"),
                            Value::variant(Value::I32(0)),
                            Value::U32(0),
                        ],
                    ),
                    Message::call(
                        &item,
                        "/StatusNotifierItem",
                        "org.kde.StatusNotifierItem",
                        "Activate",
                        vec![Value::I32(0), Value::I32(0)],
                    ),
                ] {
                    bus.send(&call).unwrap();
                }
            }
            _ => {}
        }
    }
}

fn next(events: &Receiver<Event>) -> Event {
    events
        .recv_timeout(Duration::from_secs(2))
        .expect("an event")
}

fn call(seen: &Receiver<Message>, member: &str) -> Message {
    loop {
        let message = seen.recv_timeout(Duration::from_secs(2)).expect("a call");
        if message.member == member {
            return message;
        }
    }
}

#[test]
#[ignore = "requires a private session bus: AEGLE_TEST_BUS=private dbus-run-session -- ..."]
fn services_follow_their_specifications() {
    assert_eq!(std::env::var("AEGLE_TEST_BUS").as_deref(), Ok("private"));
    let (seen_tx, seen) = channel();
    let bus = Connection::session(Duration::from_secs(1)).unwrap();
    std::thread::spawn(move || desktop(bus, seen_tx));
    std::thread::sleep(Duration::from_millis(50));
    let (tx, events) = channel();
    let app = Desktop::new("org.aegle.Test", move |event| tx.send(event).unwrap()).unwrap();

    // Notifications: our id, then the server's action and close.
    let id = app
        .notify(&Notification {
            summary: "Saved",
            body: "notes.txt",
            actions: &[("open", "Open")],
        })
        .unwrap();
    let notify = call(&seen, "Notify");
    assert_eq!(notify.body[0].as_str(), Some("org.aegle.Test"));
    assert_eq!(notify.body[5], Value::strings(["open", "Open"]));
    assert_eq!(
        next(&events),
        Event::NotificationAction {
            notification: id,
            action: "open".into()
        }
    );
    assert_eq!(
        next(&events),
        Event::NotificationClosed { notification: id }
    );

    // File dialogs: local paths are decoded; a cancelled dialog has none.
    let filters: &[(&str, &[&str])] = &[("Text", &["*.txt"])];
    let open = app
        .open_file(&FileDialog {
            title: "Open",
            filters,
            multiple: true,
            ..Default::default()
        })
        .unwrap();
    let request = call(&seen, "OpenFile");
    let options = &request.body[2];
    assert_eq!(options.get("multiple").and_then(Value::as_bool), Some(true));
    let filter = &options.get("filters").unwrap().items()[0];
    assert_eq!(filter.items()[0].as_str(), Some("Text"));
    let paths = vec![PathBuf::from("/tmp/a b.txt"), PathBuf::from("/tmp/c")];
    assert_eq!(
        next(&events),
        Event::Files {
            request: open,
            paths: Some(paths)
        }
    );
    let save = app
        .save_file(&FileDialog {
            name: "notes.txt",
            ..Default::default()
        })
        .unwrap();
    let request = call(&seen, "SaveFile");
    assert_eq!(
        request.body[2].get("current_name").and_then(Value::as_str),
        Some("notes.txt")
    );
    assert_eq!(
        next(&events),
        Event::Files {
            request: save,
            paths: None
        }
    );

    // Global shortcuts: a session, then the binding in the XDG form.
    app.bind_shortcuts(&[Shortcut {
        id: "toggle",
        description: "Show or hide",
        trigger: "Ctrl+Alt+K".parse().unwrap(),
    }])
    .unwrap();
    call(&seen, "CreateSession");
    let bind = call(&seen, "BindShortcuts");
    let shortcut = &bind.body[1].items()[0];
    assert_eq!(shortcut.items()[0].as_str(), Some("toggle"));
    assert_eq!(
        shortcut.items()[1]
            .get("preferred_trigger")
            .and_then(Value::as_str),
        Some("CTRL+ALT+k")
    );
    assert_eq!(
        next(&events),
        Event::Shortcut {
            id: "toggle".into()
        }
    );

    // Tray: registration, properties and menu as a host reads them.
    let rgba = [255, 0, 0, 128].repeat(4);
    let items = [MenuItem::Item {
        id: 30,
        label: "Mute",
        enabled: true,
        checked: Some(true),
    }];
    let menu = [
        MenuItem::Item {
            id: 10,
            label: "Show",
            enabled: true,
            checked: None,
        },
        MenuItem::Separator,
        MenuItem::Submenu {
            label: "Sound",
            items: &items,
        },
        MenuItem::Item {
            id: 20,
            label: "Quit",
            enabled: true,
            checked: None,
        },
    ];
    app.set_tray(Some(&Tray {
        icon: Icon {
            width: 2,
            height: 2,
            rgba: &rgba,
        },
        tooltip: "Test app",
        menu: &menu,
    }))
    .unwrap();
    call(&seen, "RegisterStatusNotifierItem");
    let all = seen.recv_timeout(Duration::from_secs(2)).unwrap();
    let properties = &all.body[0];
    assert_eq!(
        properties.get("Status").and_then(Value::as_str),
        Some("Active")
    );
    assert_eq!(
        properties.get("Menu").and_then(Value::as_str),
        Some("/MenuBar")
    );
    let pixmap = &properties.get("IconPixmap").unwrap().items()[0];
    assert_eq!(
        pixmap.items()[2].items()[..4],
        [128, 255, 0, 0].map(Value::Byte)
    );
    let layout = seen.recv_timeout(Duration::from_secs(2)).unwrap();
    let root = &layout.body[1];
    let children = root.items()[2].items();
    assert_eq!(children.len(), 4);
    let sound = &children[2];
    assert_eq!(
        sound.items()[1]
            .get("children-display")
            .and_then(Value::as_str),
        Some("submenu")
    );
    let mute = &sound.items()[2].items()[0];
    assert_eq!(mute.items()[0].as_i64(), Some(4));
    assert_eq!(
        mute.items()[1].get("toggle-state").and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(next(&events), Event::TrayMenu { item: 30 });
    assert_eq!(next(&events), Event::TrayActivated);
    app.set_tray(None).unwrap();
}
