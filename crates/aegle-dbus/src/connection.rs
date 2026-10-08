//! The session bus connection: authentication, Hello, sending and reading.

use std::{
    env,
    io::{self, ErrorKind, Read, Write},
    os::{
        linux::net::SocketAddrExt,
        unix::{
            fs::MetadataExt,
            net::{SocketAddr, UnixStream},
        },
    },
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use crate::{Kind, MESSAGE_LIMIT, Message};

/// An authenticated bus connection. Sending is safe from any thread; each
/// message is written whole.
pub struct Connection {
    stream: Mutex<UnixStream>,
    serial: AtomicU32,
    name: String,
}

fn invalid(text: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, text)
}

impl Connection {
    /// Connects to the session bus from `DBUS_SESSION_BUS_ADDRESS` (its first
    /// `unix:path=` or `unix:abstract=` address without percent escapes) or
    /// `$XDG_RUNTIME_DIR/bus`, authenticates with the process's credentials
    /// and says Hello, waiting at most `timeout` for each reply.
    pub fn session(timeout: Duration) -> io::Result<Self> {
        let address = match env::var("DBUS_SESSION_BUS_ADDRESS") {
            Ok(address) => address,
            Err(_) => {
                let runtime = env::var("XDG_RUNTIME_DIR").map_err(|_| ErrorKind::NotFound)?;
                format!("unix:path={runtime}/bus")
            }
        };
        Self::connect(&address, timeout)
    }

    /// [`Self::session`] for the bus at `address`, in the same form as
    /// `DBUS_SESSION_BUS_ADDRESS`.
    pub fn connect(address: &str, timeout: Duration) -> io::Result<Self> {
        let stream = open(address)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let uid = std::fs::metadata("/proc/self")?.uid().to_string();
        let hex: String = uid.bytes().map(|b| format!("{b:02x}")).collect();
        (&stream).write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
        let mut line = Vec::new();
        while !line.ends_with(b"\r\n") {
            let mut byte = [0];
            (&stream).read_exact(&mut byte)?;
            line.push(byte[0]);
            if line.len() > 256 {
                return Err(invalid("overlong authentication reply"));
            }
        }
        if !line.starts_with(b"OK ") {
            return Err(io::Error::new(ErrorKind::PermissionDenied, "bus refused"));
        }
        (&stream).write_all(b"BEGIN\r\n")?;
        let mut connection = Self {
            stream: Mutex::new(stream.try_clone()?),
            serial: AtomicU32::new(0),
            name: String::new(),
        };
        let hello = Message::call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            vec![],
        );
        let serial = connection.send(&hello)?;
        let mut incoming = Incoming::new(stream);
        loop {
            let reply = incoming.recv()?;
            if reply.reply_serial == serial {
                connection.name = (reply.body.first())
                    .and_then(|name| name.as_str())
                    .filter(|_| reply.kind == Kind::Return)
                    .ok_or_else(|| invalid("Hello failed"))?
                    .into();
                break;
            }
        }
        // Later reads come from `incoming()`, which has no timeout.
        incoming.stream.set_read_timeout(None)?;
        Ok(connection)
    }

    /// The connection's unique bus name, such as `:1.42`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sends `message` with the next serial, which it returns.
    pub fn send(&self, message: &Message) -> io::Result<u32> {
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        let bytes = message.encode(serial);
        if bytes.len() > MESSAGE_LIMIT {
            return Err(invalid("message too large"));
        }
        let stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        (&*stream).write_all(&bytes)?;
        Ok(serial)
    }

    /// A reader of the messages that arrive on this connection, blocking
    /// until one does.
    pub fn incoming(&self) -> io::Result<Incoming> {
        let stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        Ok(Incoming::new(stream.try_clone()?))
    }

    /// Calls `org.freedesktop.DBus.AddMatch` with `rule`, without waiting.
    pub fn add_match(&self, rule: &str) -> io::Result<u32> {
        self.send(&Message::call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            vec![crate::Value::str(rule)],
        ))
    }
}

/// Reads messages from one connection. Undecodable messages are skipped.
pub struct Incoming {
    stream: UnixStream,
    buffer: Vec<u8>,
}

impl Incoming {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    /// The socket, to wait on. Making it nonblocking or setting a read
    /// timeout applies to the whole connection, sending included.
    pub fn stream(&self) -> &UnixStream {
        &self.stream
    }

    /// The next message, reading until one is complete.
    pub fn recv(&mut self) -> io::Result<Message> {
        loop {
            if let Some(message) = self.take()? {
                return Ok(message);
            }
            if !self.fill()? {
                return Err(ErrorKind::UnexpectedEof.into());
            }
        }
    }

    /// Reads what is available once; false when the bus closed the
    /// connection. A nonblocking socket with nothing to read returns true.
    pub fn fill(&mut self) -> io::Result<bool> {
        let start = self.buffer.len();
        self.buffer.resize(start + 4096, 0);
        let read = (&self.stream).read(&mut self.buffer[start..]);
        self.buffer.truncate(start + *read.as_ref().unwrap_or(&0));
        match read {
            Ok(0) => Ok(false),
            Ok(_) => Ok(true),
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
            {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    /// The next complete message already read, if any.
    pub fn take(&mut self) -> io::Result<Option<Message>> {
        loop {
            let Some(header) = self.buffer.first_chunk::<16>() else {
                return Ok(None);
            };
            let length = Message::length(header).ok_or_else(|| invalid("not a message"))?;
            if self.buffer.len() < length {
                return Ok(None);
            }
            let message = Message::decode(&self.buffer[..length]);
            self.buffer.drain(..length);
            if message.is_some() {
                return Ok(message);
            }
        }
    }
}

/// Opens the first `unix:path=` or `unix:abstract=` address.
fn open(address: &str) -> io::Result<UnixStream> {
    let address = address
        .split(';')
        .filter_map(|entry| entry.strip_prefix("unix:"))
        .filter(|entry| !entry.contains('%'))
        .flat_map(|entry| entry.split(','))
        .find_map(|pair| match pair.split_once('=')? {
            ("path", path) => SocketAddr::from_pathname(path).ok(),
            ("abstract", name) => SocketAddr::from_abstract_name(name).ok(),
            _ => None,
        })
        .ok_or(ErrorKind::NotFound)?;
    UnixStream::connect_addr(&address)
}
