//! Native Windows 11 windows without a polling loop or widget dependency.
//!
//! `Win32` implements the [`aegle_platform::Platform`] and
//! [`aegle_platform::SurfacePlatform`] contracts. Software presentation
//! borrows one bounded BGRA8 buffer and uploads an opaque DIB; GPU hosts use
//! an owned window-surface lease instead. Input methods compose through a Text
//! Services Framework text store that holds the focused editor's surrounding
//! text, so they can predict from it and reconvert it. Windows has one input
//! seat, `SeatId(0)`, and no layer surfaces.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod drag;
#[cfg(windows)]
mod error;
#[cfg(windows)]
mod ime_edit;
#[cfg(windows)]
mod ime_types;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod preferences;
#[cfg(windows)]
mod procedure;
#[cfg(windows)]
mod software;
#[cfg(windows)]
mod text_store;
#[cfg(windows)]
mod tsf;
#[cfg(windows)]
mod window;

#[cfg(windows)]
pub use window::{Win32, WindowSurface};

#[cfg(windows)]
use aegle_platform::{
    Event, ImeEvent, ImeRequest, ImeUpdate, Modifiers, PixelSize, PointerButton, PointerKind,
    Preedit, Preferences, SeatId, WindowId, WindowInfo, WindowOptions,
};
#[cfg(windows)]
use error::Error;

/// The only input seat on Windows.
#[cfg(windows)]
const SEAT: SeatId = SeatId(0);
