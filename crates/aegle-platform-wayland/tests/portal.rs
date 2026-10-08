//! Appearance preferences come from a desktop portal's Settings interface:
//! read at connection, then followed through SettingChanged. Run on a private
//! compositor with a private session bus, which a mock portal joins:
//! `dbus-run-session -- cargo test -p aegle-platform-wayland --test portal -- --ignored`.

use std::time::{Duration, Instant};

use aegle_dbus::{Connection, Message, Value};
use aegle_platform_wayland::{Event, Wayland};

const SETTINGS: &str = "org.freedesktop.portal.Settings";

/// Owns the portal name, answers ReadOne with a dark color scheme and no
/// other values, then announces a contrast request once the client has read.
fn mock_portal(bus: Connection) {
    let mut incoming = bus.incoming().unwrap();
    let own = Message::call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        vec![Value::str("org.freedesktop.portal.Desktop"), Value::U32(4)],
    );
    let serial = bus.send(&own).unwrap();
    while incoming.recv().unwrap().reply_serial != serial {}
    let mut reads = 0;
    loop {
        let call = incoming.recv().unwrap();
        if !call.is_call(SETTINGS, "ReadOne") {
            continue;
        }
        let key = call.body[1].as_str().unwrap().to_owned();
        let reply = match key.as_str() {
            "color-scheme" => call.reply(vec![Value::variant(Value::U32(1))]),
            _ => call.error("org.freedesktop.portal.Error.NotFound", "unset"),
        };
        bus.send(&reply).unwrap();
        reads += 1;
        if reads == 5 {
            // After the platform's startup read has returned.
            std::thread::sleep(Duration::from_millis(200));
            let changed = Message::signal(
                "/org/freedesktop/portal/desktop",
                SETTINGS,
                "SettingChanged",
                vec![
                    Value::str("org.freedesktop.appearance"),
                    Value::str("contrast"),
                    Value::variant(Value::U32(1)),
                ],
            );
            bus.send(&changed).unwrap();
            return;
        }
    }
}

#[test]
#[ignore = "requires a private compositor and a private session bus"]
fn preferences_follow_the_settings_portal() {
    let bus = Connection::session(Duration::from_secs(1)).unwrap();
    let portal = std::thread::spawn(move || mock_portal(bus));
    // The portal must own its name before the platform reads.
    std::thread::sleep(Duration::from_millis(50));
    let mut platform = Wayland::connect().unwrap();
    assert_eq!(platform.preferences().dark, Some(true));
    assert_eq!(platform.preferences().high_contrast, None);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        platform.dispatch(Some(Duration::from_millis(50))).unwrap();
        if let Some(Event::Preferences(preferences)) = platform.next_event() {
            assert_eq!(preferences.high_contrast, Some(true));
            assert_eq!(preferences.dark, Some(true));
            break;
        }
        assert!(Instant::now() < deadline, "no SettingChanged preferences");
    }
    portal.join().unwrap();
}
