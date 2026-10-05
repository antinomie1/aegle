//! Event-driven Wayland windows with bounded, directly borrowed SHM framebuffers.
//!
//! This backend owns one connection and dispatch queue. It does not own widgets,
//! a renderer, fonts or an editor. Call [`Wayland::dispatch`] when idle and drain
//! [`Wayland::next_event`]; present only after a [`Event::Redraw`] notification.

mod buffers;
mod error;
mod events;
#[cfg(feature = "gpu")]
mod gpu;
mod ime;
mod ime_types;
mod input;
mod state;
mod window;

pub use error::{Error, PresentError};
pub use events::{Event, PixelSize, WindowId, WindowInfo, WindowOptions};
#[cfg(feature = "gpu")]
pub use gpu::WindowSurface;
pub use ime_types::{ImeCause, ImeEvent, ImeHints, ImePurpose, ImeRequest, ImeUpdate, Preedit};
pub use smithay_client_toolkit::seat::{
    keyboard::{KeyEvent, Keysym, Modifiers},
    pointer::PointerEventKind,
};
pub use wayland_client::protocol::wl_seat::WlSeat;
pub use window::{WakeHandle, Wayland};

use state::State;
