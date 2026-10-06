//! Native Windows 11 windows without a polling loop or widget dependency.
//!
//! Software presentation borrows one bounded RGBA8 buffer and uploads an opaque
//! DIB. Vulkan hosts use an owned window-surface lease instead. IME composition
//! uses the native IMM compatibility interface; this is not a TSF text store and
//! does not provide surrounding-text reconversion or the touch-keyboard contract.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
mod clipboard;
mod error;
mod events;
#[cfg(windows)]
mod ime;
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
mod window;

pub use aegle_types::Preferences;
pub use error::{Error, PresentError};
pub use events::{Event, Modifiers, PixelSize, PointerKind, WindowId, WindowInfo, WindowOptions};
pub use ime_types::{ImeEvent, ImeRequest, ImeUpdate, Preedit, utf16_cursor};
#[cfg(windows)]
pub use window::{WakeHandle, Win32, WindowSurface};
