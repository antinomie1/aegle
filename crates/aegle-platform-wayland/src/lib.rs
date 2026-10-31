//! Event-driven Wayland windows with bounded, directly borrowed SHM framebuffers.
//!
//! [`Wayland`] implements the [`aegle_platform::Platform`] contract on one
//! connection and dispatch queue, and [`aegle_platform::SurfacePlatform`]
//! with the `gpu` feature. It does not own widgets, a renderer, fonts or an
//! editor. `WindowOptions::layer` creates a wlr layer surface on the same path;
//! text input uses text-input-v3 and the clipboard and drag and drop one data
//! device per seat.

mod buffers;
mod clipboard;
mod drag;
mod events;
#[cfg(feature = "gpu")]
mod gpu;
mod ime;
mod ime_types;
mod input;
mod platform;
mod scale;
mod state;
mod window;

pub use clipboard::CLIPBOARD_LIMIT;
#[cfg(feature = "gpu")]
pub use gpu::WindowSurface;
pub use window::Wayland;

use aegle_platform::{
    Error, Event, ImeEvent, PixelSize, Preferences, SeatId, TouchPhase, WindowId, WindowInfo,
    WindowOptions,
};
use state::State;
use wayland_client::protocol::wl_seat::WlSeat;
