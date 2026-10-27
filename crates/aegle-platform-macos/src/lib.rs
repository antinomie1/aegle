//! Native macOS windows on an AppKit event loop without a polling timer.
//!
//! `MacOs` implements the [`aegle_platform::Platform`] and
//! [`aegle_platform::SurfacePlatform`] contracts. The application, its windows
//! and every callback live on the process's main thread; the loop pulls events
//! with `nextEventMatchingMask:` instead of handing control to `NSApplication
//! run`, so [`aegle_platform::Platform::dispatch`] keeps its blocking,
//! deadline-bounded contract. Software presentation borrows one bounded BGRA8
//! buffer and shows it as an opaque `CGImage` from `drawRect:`; GPU hosts lease
//! the window's `NSView` instead, which `wgpu` turns into a `CAMetalLayer`.
//! Text input goes through `NSTextInputClient`, so dead keys, input methods and
//! the character palette share one path and the editor's surrounding text.
//! macOS has one input seat, `SeatId(0)`, and no layer surfaces.
//!
//! Views use a bottom-left origin, as AppKit's own coordinates do, and this
//! crate converts to the contract's top-left logical coordinates at the edge.
//!
//! Everything above compiles only on macOS. [`Composition`], the model that
//! turns `NSTextInputClient` operations into [`aegle_platform::ImeUpdate`]s
//! and answers its UTF-16 range queries, is platform-independent so it can be
//! tested anywhere; the AppKit glue in `ime_client` only forwards to it.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod clipboard;
#[cfg(target_os = "macos")]
mod drag;
#[cfg(target_os = "macos")]
mod frame;
mod ime;
#[cfg(target_os = "macos")]
mod ime_client;
mod ime_text;
#[cfg(target_os = "macos")]
mod input;
#[cfg(target_os = "macos")]
mod keys;
#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "macos")]
mod preferences;
#[cfg(target_os = "macos")]
mod shared;
#[cfg(target_os = "macos")]
mod view;
#[cfg(target_os = "macos")]
mod wake;
#[cfg(target_os = "macos")]
mod window;

pub use ime::{Composition, KeyOutcome, Marked};
#[cfg(target_os = "macos")]
pub use window::{MacOs, WindowSurface};

/// The only input seat on macOS.
#[cfg(target_os = "macos")]
const SEAT: aegle_platform::SeatId = aegle_platform::SeatId(0);
