//! Minimal session-bus client for the XDG desktop portal appearance settings.
//!
//! It authenticates with EXTERNAL credentials, reads `color-scheme`, `contrast`
//! and `reduced-motion` with a short bounded wait, then follows `SettingChanged`
//! signals from the event loop. Only the few message shapes involved are
//! encoded or decoded; anything else is ignored. A missing bus or portal leaves
//! preferences unknown instead of failing the Wayland connection.

use crate::Preferences;
use std::{
    env,
    io::{ErrorKind, Read, Write},
    os::{
        linux::net::SocketAddrExt,
        unix::{
            fs::MetadataExt,
            net::{SocketAddr, UnixStream},
        },
    },
    time::{Duration, Instant},
};

const NAMESPACE: &str = "org.freedesktop.appearance";
const KEYS: [&str; 3] = ["color-scheme", "contrast", "reduced-motion"];
/// Serial of the first ReadOne request; the others follow in `KEYS` order.
const FIRST_READ: u32 = 3;
/// Larger messages are not settings traffic; the connection is dropped.
const MESSAGE_LIMIT: usize = 64 * 1024;
/// Bounds startup when the portal is slow to activate; later replies still apply.
const STARTUP_WAIT: Duration = Duration::from_millis(100);

/// Decoder state for one bus connection.
pub(crate) struct Portal {
    buffer: Vec<u8>,
    /// Bit per `KEYS` entry still awaiting its ReadOne reply.
    pending: u8,
}

/// Connects, requests all settings and applies replies arriving within the
/// startup bound. Returns a nonblocking stream for the event loop.
pub(crate) fn connect(preferences: &mut Preferences) -> Option<(UnixStream, Portal)> {
    let stream = bus()?;
    stream.set_read_timeout(Some(STARTUP_WAIT)).ok()?;
    stream.set_write_timeout(Some(STARTUP_WAIT)).ok()?;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    let hex: String = uid
        .to_string()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    (&stream)
        .write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())
        .ok()?;
    let mut line = Vec::new();
    while !line.ends_with(b"\r\n") {
        let mut byte = [0];
        (&stream).read_exact(&mut byte).ok()?;
        line.push(byte[0]);
        if line.len() > 256 {
            return None;
        }
    }
    if !line.starts_with(b"OK ") {
        return None;
    }
    let mut out = b"BEGIN\r\n".to_vec();
    let (bus, bus_path) = ("org.freedesktop.DBus", "/org/freedesktop/DBus");
    call(&mut out, 1, bus, bus_path, bus, "Hello", &[]);
    let rule = format!(
        "type='signal',interface='org.freedesktop.portal.Settings',\
         member='SettingChanged',arg0='{NAMESPACE}'"
    );
    call(&mut out, 2, bus, bus_path, bus, "AddMatch", &[&rule]);
    for (serial, key) in (FIRST_READ..).zip(KEYS) {
        call(
            &mut out,
            serial,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            &[NAMESPACE, key],
        );
    }
    (&stream).write_all(&out).ok()?;
    let mut portal = Portal {
        buffer: Vec::new(),
        pending: (1 << KEYS.len()) - 1,
    };
    let deadline = Instant::now() + STARTUP_WAIT;
    while portal.pending != 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            break;
        }
        if !portal.read(&stream, preferences) {
            return None;
        }
    }
    stream.set_nonblocking(true).ok()?;
    Some((stream, portal))
}

impl Portal {
    /// Reads available bytes and applies complete messages. Returns false when
    /// the connection closed, failed or sent an oversized message.
    pub fn read(&mut self, mut stream: &UnixStream, preferences: &mut Preferences) -> bool {
        let start = self.buffer.len();
        self.buffer.resize(start + 4096, 0);
        let read = stream.read(&mut self.buffer[start..]);
        self.buffer.truncate(start + *read.as_ref().unwrap_or(&0));
        match read {
            Ok(0) => return false,
            Ok(_) => {}
            Err(error) => {
                return matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                );
            }
        }
        while self.buffer.len() >= 16 {
            if !matches!(self.buffer[0], b'l' | b'B') {
                return false;
            }
            let mut header = Reader::new(&self.buffer);
            header.pos = 4;
            let (Some(body), Some(_), Some(fields)) = (header.u32(), header.u32(), header.u32())
            else {
                return false;
            };
            let total = (body as usize).saturating_add((fields as usize).next_multiple_of(8));
            if total > MESSAGE_LIMIT {
                return false;
            }
            if self.buffer.len() < 16 + total {
                break;
            }
            apply(&self.buffer[..16 + total], &mut self.pending, preferences);
            self.buffer.drain(..16 + total);
        }
        true
    }
}

/// Applies one ReadOne reply or SettingChanged signal; ignores other messages.
fn apply(message: &[u8], pending: &mut u8, preferences: &mut Preferences) -> Option<()> {
    let mut reader = Reader::new(message);
    reader.pos = 12;
    let end = 16 + reader.u32()? as usize;
    let (mut reply, mut member, mut signature) = (None, None, "");
    while reader.pos < end {
        reader.align(8);
        let code = reader.byte()?;
        match (code, reader.signature()?) {
            (3, "s") => member = Some(reader.string()?),
            (5, "u") => reply = Some(reader.u32()?),
            (8, "g") => signature = reader.signature()?,
            (_, kind) if kind.len() == 1 => reader.skip(kind.as_bytes()[0])?,
            _ => return None,
        }
    }
    reader.align(8);
    let (key, value) = match message[1] {
        // A method return or error answering one of our ReadOne calls.
        2 | 3 => {
            let index = reply?.checked_sub(FIRST_READ)? as usize;
            if index >= KEYS.len() || *pending & (1 << index) == 0 {
                return None;
            }
            *pending &= !(1 << index);
            let value = (message[1] == 2 && signature == "v")
                .then(|| reader.variant_u32())
                .flatten();
            (KEYS[index], value)
        }
        4 if member == Some("SettingChanged") && signature == "ssv" => {
            if reader.string()? != NAMESPACE {
                return None;
            }
            let key = reader.string()?;
            (key, reader.variant_u32())
        }
        _ => return None,
    };
    // Portal values: color-scheme 1 dark, 2 light, 0 no preference;
    // contrast and reduced-motion 1 means requested.
    match key {
        "color-scheme" => {
            preferences.dark = value.and_then(|v| (v == 1 || v == 2).then_some(v == 1))
        }
        "contrast" => preferences.high_contrast = value.map(|v| v == 1),
        "reduced-motion" => preferences.reduced_motion = value.map(|v| v == 1),
        _ => {}
    }
    Some(())
}

/// Opens the first `unix:path=` or `unix:abstract=` session bus address, or
/// `$XDG_RUNTIME_DIR/bus`. Percent-escaped addresses are not supported.
fn bus() -> Option<UnixStream> {
    let Ok(address) = env::var("DBUS_SESSION_BUS_ADDRESS") else {
        let path = std::path::Path::new(&env::var_os("XDG_RUNTIME_DIR")?).join("bus");
        return UnixStream::connect(path).ok();
    };
    address.split(';').find_map(|entry| {
        let entry = entry.strip_prefix("unix:")?;
        if entry.contains('%') {
            return None;
        }
        entry.split(',').find_map(|pair| {
            let address = match pair.split_once('=')? {
                ("path", path) => SocketAddr::from_pathname(path).ok()?,
                ("abstract", name) => SocketAddr::from_abstract_name(name).ok()?,
                _ => return None,
            };
            UnixStream::connect_addr(&address).ok()
        })
    })
}

/// Appends a little-endian method call whose arguments are all strings.
fn call(
    out: &mut Vec<u8>,
    serial: u32,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    args: &[&str],
) {
    let mut body = Vec::new();
    for arg in args {
        string(&mut body, arg);
    }
    let mut message = vec![b'l', 1, 0, 1];
    message.extend((body.len() as u32).to_le_bytes());
    message.extend(serial.to_le_bytes());
    message.extend([0; 4]);
    let signature = "s".repeat(args.len());
    for (code, kind, value) in [
        (1, b'o', path),
        (2, b's', interface),
        (3, b's', member),
        (6, b's', destination),
        (8, b'g', &signature),
    ] {
        if value.is_empty() {
            continue;
        }
        message.resize(message.len().next_multiple_of(8), 0);
        message.extend([code, 1, kind, 0]);
        if kind == b'g' {
            message.push(value.len() as u8);
            message.extend(value.as_bytes());
            message.push(0);
        } else {
            string(&mut message, value);
        }
    }
    let fields = (message.len() - 16) as u32;
    message[12..16].copy_from_slice(&fields.to_le_bytes());
    message.resize(message.len().next_multiple_of(8), 0);
    message.extend(body);
    out.extend(message);
}

fn string(out: &mut Vec<u8>, value: &str) {
    out.resize(out.len().next_multiple_of(4), 0);
    out.extend((value.len() as u32).to_le_bytes());
    out.extend(value.as_bytes());
    out.push(0);
}

/// Bounds-checked reader; alignment is relative to the message start.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    big: bool,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            big: data[0] == b'B',
        }
    }
    fn align(&mut self, to: usize) {
        self.pos = self.pos.next_multiple_of(to);
    }
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.pos..self.pos.checked_add(len)?)?;
        self.pos += len;
        Some(bytes)
    }
    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        self.align(4);
        let bytes = self.take(4)?.try_into().ok()?;
        Some(if self.big {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        })
    }
    fn text(&mut self, len: usize) -> Option<&'a str> {
        let text = std::str::from_utf8(self.take(len)?).ok()?;
        self.take(1)?;
        Some(text)
    }
    fn string(&mut self) -> Option<&'a str> {
        let len = self.u32()? as usize;
        self.text(len)
    }
    fn signature(&mut self) -> Option<&'a str> {
        let len = self.byte()? as usize;
        self.text(len)
    }
    fn variant_u32(&mut self) -> Option<u32> {
        (self.signature()? == "u").then(|| self.u32()).flatten()
    }
    /// Skips one basic header value.
    fn skip(&mut self, kind: u8) -> Option<()> {
        match kind {
            b's' | b'o' => self.string().map(drop),
            b'g' => self.signature().map(drop),
            b'u' | b'i' | b'b' | b'h' => self.u32().map(drop),
            b'y' => self.byte().map(drop),
            _ => None,
        }
    }
}
