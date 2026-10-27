//! Accessibility without a second application tree or UI-thread polling.
//!
//! Hosts derive AccessKit updates from their retained state, and route returned
//! actions through the same controls as physical input. The mailbox bridges
//! platform callbacks to the owning UI thread; it does not execute UI callbacks.
//! OS adapters and their additional caches/threads are explicitly opt-in:
//! `unix` (AT-SPI), `windows` (UI Automation) and `macos` (NSAccessibility).

#[cfg(all(feature = "macos", target_os = "macos"))]
mod macos;
mod mailbox;
#[cfg(all(feature = "unix", unix, not(target_os = "macos")))]
mod unix;
#[cfg(all(feature = "windows", windows))]
mod windows;

pub use accesskit;
#[cfg(all(feature = "macos", target_os = "macos"))]
pub use macos::MacosAdapter;
pub use mailbox::{Event, Handlers, Mailbox};
#[cfg(all(feature = "unix", unix, not(target_os = "macos")))]
pub use unix::UnixAdapter;
#[cfg(all(feature = "windows", windows))]
pub use windows::WindowsAdapter;
