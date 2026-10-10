//! Native Windows 11 windows without a polling loop or widget dependency.
//!
//! Software presentation borrows one bounded RGBA8 buffer and uploads an opaque
//! DIB. Vulkan hosts use an owned window-surface lease instead. Input methods
//! compose through a Text Services Framework text store that holds the
//! focused editor's surrounding text, so they can predict from it and
//! reconvert it.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod drag;
mod error;
mod events;
#[cfg(windows)]
mod ime_edit;
mod ime_types;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod native;
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

pub use aegle_types::{DragData, PointerButton, Preferences};
pub use error::{Error, PresentError};
pub use events::{Event, Modifiers, PixelSize, PointerKind, WindowId, WindowInfo, WindowOptions};
pub use ime_types::{ImeEvent, ImeRequest, ImeUpdate, MAX_SURROUNDING, Preedit};
#[cfg(windows)]
pub use window::{WakeHandle, Win32, WindowSurface};
