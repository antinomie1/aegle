//! Accessibility without a second application tree or UI-thread polling.
//!
//! Hosts derive AccessKit updates from their retained state, and route returned
//! actions through the same controls as physical input. The mailbox bridges
//! platform callbacks to the owning UI thread; it does not execute UI callbacks.
//! OS adapters and their additional caches/threads are explicitly opt-in.

mod mailbox;
#[cfg(all(feature = "unix", unix, not(target_os = "macos")))]
mod unix;

pub use accesskit;
pub use mailbox::{Event, Handlers, Mailbox};
#[cfg(all(feature = "unix", unix, not(target_os = "macos")))]
pub use unix::UnixAdapter;
