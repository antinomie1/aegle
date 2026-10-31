//! Appearance preferences come from a desktop portal's Settings interface:
//! read at connection, then followed through SettingChanged. Run on a private
//! session bus, which a mock portal joins:
//! `dbus-run-session -- cargo test -p aegle-portal --test portal -- --ignored`.

use std::time::{Duration, Instant};

use aegle_dbus::{Connection, Message, Value};
use aegle_portal::connect;
use aegle_types::Preferences;

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
            // After the client's startup read has returned.
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
#[ignore = "requires a private session bus"]
fn preferences_follow_the_settings_portal() {
    let bus = Connection::session(Duration::from_secs(1)).unwrap();
    let server = std::thread::spawn(move || mock_portal(bus));
    // The portal must own its name before the client reads.
    std::thread::sleep(Duration::from_millis(50));
    let mut preferences = Preferences::default();
    let (stream, mut portal) = connect(&mut preferences).unwrap();
    assert_eq!(preferences.dark, Some(true));
    assert_eq!(preferences.high_contrast, None);
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while preferences.high_contrast.is_none() {
        assert!(portal.read(&mut preferences), "the bus closed");
        assert!(Instant::now() < deadline, "no SettingChanged preferences");
    }
    assert_eq!(preferences.dark, Some(true));
    server.join().unwrap();
}
