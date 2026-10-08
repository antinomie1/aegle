//! A small D-Bus client over Unix sockets, without dependencies; empty on
//! targets other than Linux.
//!
//! [`Connection::session`] authenticates to the session bus and says Hello;
//! [`Connection::send`] writes a [`Message`] from any thread, and an
//! [`Incoming`] reads messages from a clone of the socket, blocking or
//! nonblocking. [`Value`] covers the basic and container types that portals,
//! notifications and status items use: no Unix file descriptors, and no 16-bit
//! integers. A message using them is skipped as undecodable.
//!
//! ```no_run
//! use aegle_dbus::{Connection, Message, Value};
//! let bus = Connection::session(std::time::Duration::from_secs(1))?;
//! let mut incoming = bus.incoming()?;
//! let call = Message::call(
//!     "org.freedesktop.DBus",
//!     "/org/freedesktop/DBus",
//!     "org.freedesktop.DBus",
//!     "GetId",
//!     vec![],
//! );
//! let serial = bus.send(&call)?;
//! loop {
//!     let reply = incoming.recv()?;
//!     if reply.reply_serial == serial {
//!         println!("bus id {:?}", reply.body.first().and_then(Value::as_str));
//!         break;
//!     }
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

#![cfg(target_os = "linux")]

mod connection;
mod message;
mod value;

pub use connection::{Connection, Incoming};
pub use message::{Kind, Message};
pub use value::Value;

/// Largest message, header and body, that is sent or accepted.
pub const MESSAGE_LIMIT: usize = 4 << 20;
