//! SDL3 windows behind the [`aegle_platform::Platform`] contract.
//!
//! [`Sdl3`] implements the contract on one SDL video connection: windows,
//! keyboard, pointer, touch and drag input, text input, the clipboard, cursors
//! and software presentation. With the `gpu` feature it also lends native
//! window handles to GPU presenters. It owns no controls, fonts, renderer or
//! editor.
//!
//! The distribution's `libSDL3.so.0` is opened at run time with `libloading`,
//! like the system text stack: nothing is linked at build time and no header is
//! read, so a machine without SDL3 fails [`aegle_platform::Platform::connect`]
//! with a `Backend` error instead of failing the build.
//!
//! SDL3 imposes three limits this backend reports rather than hides:
//!
//! - There are no desktop-shell layers: a [`aegle_platform::WindowOptions`]
//!   with a layer fails with [`aegle_platform::Error::Unsupported`].
//! - An application cannot begin a drag, only receive one.
//! - Composition is SDL's own model: the preedit, its selection and the
//!   committed text are reported, but surrounding text and the editor's
//!   selection cannot be published to the input method, and SDL reports no
//!   per-surface frame callback, so software frames are paced by the display's
//!   refresh rate instead.

mod events;
#[cfg(feature = "gpu")]
mod gpu;
mod keys;
mod platform;
mod sys;
mod text;
mod window;

#[cfg(feature = "gpu")]
pub use gpu::WindowSurface;
pub use window::{SEAT, Sdl3};
