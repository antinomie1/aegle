//! Appearance preferences from the XDG desktop portal.
//!
//! Wayland has no protocol for the user's color scheme, contrast, motion,
//! text size or double-click time, so desktop applications ask the
//! `org.freedesktop.portal.Settings` interface on the session bus. This crate
//! reads those keys once with a short bounded wait and then follows
//! `SettingChanged` signals, so a platform that owns a Unix socket can wait on
//! it with its own event loop.
//!
//! Reads are a convenience, not a requirement: a session without a bus or a
//! portal leaves the corresponding [`Preferences`] fields at `None` instead of
//! failing. The caller decides how to wait on the returned socket and when to
//! apply [`Portal::read`]'s changes to the interface.

use aegle_dbus::{Connection, Incoming, Kind, Message, Value};
use aegle_types::Preferences;
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
pub const STARTUP_WAIT: Duration = Duration::from_millis(100);

/// A subscription to the portal's appearance settings.
pub struct Portal {
    incoming: Incoming,
    /// The serial of each `KEYS` entry's ReadOne call, 0 once answered.
    pending: [u32; KEYS.len()],
}

/// Connects, requests every setting and applies replies arriving within
/// [`STARTUP_WAIT`]. Returns the socket to wait on for later changes, or
/// `None` when there is no session bus or no portal to ask.
pub fn connect(preferences: &mut Preferences) -> Option<(UnixStream, Portal)> {
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
    /// Reads available bytes and applies complete messages to `preferences`.
    /// Returns false when the connection closed, failed or sent a malformed
    /// message; the subscription is over then. On a nonblocking socket with
    /// nothing to read this returns true immediately, on a blocking one it
    /// waits for the next message.
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
