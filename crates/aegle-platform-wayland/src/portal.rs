//! XDG desktop portal appearance settings over the session bus.
//!
//! Reads `color-scheme`, `contrast`, `reduced-motion` and GNOME's
//! `text-scaling-factor` and `double-click` with a short bounded wait, then
//! follows `SettingChanged` signals from the event loop. A missing bus or
//! portal leaves preferences unknown instead of failing the Wayland connection.

use crate::Preferences;
use aegle_dbus::{Connection, Incoming, Kind, Message, Value};
use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

const NAMESPACES: [&str; 3] = [
    "org.freedesktop.appearance",
    "org.gnome.desktop.interface",
    "org.gnome.desktop.peripherals.mouse",
];
const KEYS: [(&str, &str); 5] = [
    (NAMESPACES[0], "color-scheme"),
    (NAMESPACES[0], "contrast"),
    (NAMESPACES[0], "reduced-motion"),
    (NAMESPACES[1], "text-scaling-factor"),
    (NAMESPACES[2], "double-click"),
];
/// Bounds startup when the portal is slow to activate; later replies still apply.
const STARTUP_WAIT: Duration = Duration::from_millis(100);

/// The bus connection's reading end and the ReadOne calls still unanswered.
pub(crate) struct Portal {
    incoming: Incoming,
    /// The serial of each `KEYS` entry's ReadOne call, 0 once answered.
    pending: [u32; KEYS.len()],
}

/// Connects, requests all settings and applies replies arriving within the
/// startup bound. Returns a socket for the event loop to wait on.
pub(crate) fn connect(preferences: &mut Preferences) -> Option<(UnixStream, Portal)> {
    let bus = Connection::session(STARTUP_WAIT).ok()?;
    for namespace in NAMESPACES {
        let rule = format!(
            "type='signal',interface='org.freedesktop.portal.Settings',\
             member='SettingChanged',arg0='{namespace}'"
        );
        bus.add_match(&rule).ok()?;
    }
    let mut pending = [0; KEYS.len()];
    for (serial, (namespace, key)) in pending.iter_mut().zip(KEYS) {
        let call = Message::call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            vec![Value::str(namespace), Value::str(key)],
        );
        *serial = bus.send(&call).ok()?;
    }
    let mut portal = Portal {
        incoming: bus.incoming().ok()?,
        pending,
    };
    let deadline = Instant::now() + STARTUP_WAIT;
    while portal.pending.iter().any(|&serial| serial != 0) {
        let left = deadline.saturating_duration_since(Instant::now());
        let stream = portal.incoming.stream();
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            break;
        }
        if !portal.read(preferences) {
            return None;
        }
    }
    let stream = portal.incoming.stream();
    stream.set_nonblocking(true).ok()?;
    Some((stream.try_clone().ok()?, portal))
}

impl Portal {
    /// Reads available bytes and applies complete messages. Returns false when
    /// the connection closed, failed or sent a malformed message.
    pub fn read(&mut self, preferences: &mut Preferences) -> bool {
        if !matches!(self.incoming.fill(), Ok(true)) {
            return false;
        }
        loop {
            match self.incoming.take() {
                Ok(Some(message)) => self.apply(&message, preferences),
                Ok(None) => return true,
                Err(_) => return false,
            }
        }
    }

    /// Applies one ReadOne reply or SettingChanged signal; ignores other messages.
    fn apply(&mut self, message: &Message, preferences: &mut Preferences) {
        let (index, value) = match message.kind {
            Kind::Return | Kind::Error => {
                let serial = message.reply_serial;
                let Some(index) = self.pending.iter().position(|&s| s == serial && s != 0) else {
                    return;
                };
                self.pending[index] = 0;
                (index, message.body.first().and_then(Value::as_f64))
            }
            Kind::Signal
                if message.is_signal("org.freedesktop.portal.Settings", "SettingChanged") =>
            {
                let [namespace, key, value] = &message.body[..] else {
                    return;
                };
                let entry = (namespace.as_str(), key.as_str());
                let Some(index) = KEYS.iter().position(|&(n, k)| entry == (Some(n), Some(k)))
                else {
                    return;
                };
                (index, value.as_f64())
            }
            _ => return,
        };
        // Portal values: color-scheme 1 dark, 2 light, 0 no preference;
        // contrast and reduced-motion 1 means requested; the scaling factor is a
        // double and the double-click time an int32 of milliseconds.
        match KEYS[index].1 {
            "color-scheme" => {
                preferences.dark = value.and_then(|v| (v == 1.0 || v == 2.0).then_some(v == 1.0))
            }
            "contrast" => preferences.high_contrast = value.map(|v| v == 1.0),
            "reduced-motion" => preferences.reduced_motion = value.map(|v| v == 1.0),
            "double-click" => {
                preferences.double_click = value
                    .filter(|v| (100.0..=5000.0).contains(v))
                    .map(|v| Duration::from_millis(v as u64))
            }
            _ => {
                preferences.text_scale = value
                    .filter(|v| (0.5..=4.0).contains(v))
                    .map(|v| (v * 100.0).round() as u16)
            }
        }
    }
}
