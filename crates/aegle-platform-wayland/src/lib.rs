//! Event-driven Wayland windows with bounded, directly borrowed SHM framebuffers.
//!
//! This backend owns one connection and dispatch queue. It does not own widgets,
//! a renderer, fonts or an editor. Call [`Wayland::dispatch`] when idle and drain
//! [`Wayland::next_event`]; present only after a [`Event::Redraw`] notification.
//! [`WindowOptions::layer`] creates a wlr layer surface on the same path.

mod buffers;
mod clipboard;
mod error;
mod events;
#[cfg(feature = "gpu")]
mod gpu;
mod ime;
mod ime_types;
mod input;
mod portal;
mod scale;
mod state;
mod window;

pub use aegle_types::{Preferences, TouchPhase};
pub use clipboard::CLIPBOARD_LIMIT;
pub use error::{Error, PresentError};
pub use events::{Event, LayerOptions, PixelSize, WindowId, WindowInfo, WindowOptions};
#[cfg(feature = "gpu")]
pub use gpu::WindowSurface;
pub use ime_types::{ImeCause, ImeEvent, ImeHints, ImePurpose, ImeRequest, ImeUpdate, Preedit};
pub use smithay_client_toolkit::{
    seat::{
        keyboard::{KeyEvent, Keysym, Modifiers},
        pointer::PointerEventKind,
    },
    shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer},
};
pub use wayland_client::protocol::wl_seat::WlSeat;
pub use window::{WakeHandle, Wayland};

use state::State;
