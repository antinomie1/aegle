//! The contract every native Aegle platform implements.
//!
//! A platform owns one connection to the windowing system, its windows and a
//! blocking, poll-free event loop. It translates native input into the shared
//! vocabulary here (logical coordinates, [`Key`], [`Modifiers`], seats, IME
//! transactions), so a host such as `aegle-app` is written once against
//! [`Platform`] and a new platform adds an implementation without changing the
//! host. Platforms own no controls, fonts or renderer.
//!
//! The host drives one loop on the UI thread: [`Platform::dispatch`] waits,
//! [`Platform::next_event`] drains the queue, and presentation follows a
//! [`Event::Redraw`]. A capability the platform cannot provide fails with
//! [`Error::Unsupported`] or [`Error::ImeUnavailable`] instead of pretending to
//! succeed.

mod error;
mod event;
mod ime;
mod window;

pub use aegle_types::{
    Cursor, DragData, Key, Modifiers, PixelRect, Point, PointerButton, Preferences, Rect, Region,
    TouchPhase,
};
pub use error::{DrawResult, Error, Result};
pub use event::{Event, PointerKind};
pub use ime::{ImeEvent, ImeRequest, ImeUpdate, MAX_SURROUNDING, Preedit};
pub use window::{
    Anchor, KeyboardInteractivity, Layer, LayerOptions, PixelSize, SeatId, WakeHandle, WindowId,
    WindowInfo, WindowOptions,
};

use std::time::Duration;

/// A native windowing system connection, used from the UI thread only.
///
/// Every method that names a window fails with [`Error::InvalidWindow`] once
/// that window is removed. Methods that name a seat ignore a seat that has
/// disappeared, whose removal the event queue already reports.
pub trait Platform: Sized {
    /// Connects to the windowing system named by the environment.
    fn connect() -> Result<Self>;

    /// System appearance preferences known so far; changes arrive as
    /// [`Event::Preferences`].
    fn preferences(&self) -> Preferences;

    /// A cloneable, thread-safe signal that queues [`Event::Wake`]. Handles
    /// share one native resource, created on first use.
    fn wake_handle(&mut self) -> Result<WakeHandle>;

    /// Whether native composition exists, so [`Self::configure_ime`] with a
    /// request can succeed.
    fn ime_available(&self) -> bool;

    /// Creates a window that stays hidden or unconfigured until its first
    /// [`Event::Configure`]. Layer surfaces fail with [`Error::Unsupported`]
    /// where the platform has none.
    fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId>;

    /// Destroys a window and drops its queued events. Identities are never reused.
    fn remove_window(&mut self, window: WindowId) -> Result<()>;

    /// The latest configured geometry and activation.
    fn window_info(&self, window: WindowId) -> Result<WindowInfo>;

    /// Waits at most `timeout` (`None`: indefinitely) for native work and
    /// translates it into queued events. Returns at once while events are queued.
    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()>;

    /// Pops the oldest queued event. Drain the queue before dispatching again.
    fn next_event(&mut self) -> Option<Event>;

    /// Marks a window's content changed. Requests coalesce into one
    /// [`Event::Redraw`] when the window can present.
    fn request_redraw(&mut self, window: WindowId) -> Result<()>;

    /// Sets the pointer shape over a window's content, applied to pointers
    /// inside it now and on every later entry.
    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()>;

    /// Enables, updates or (with `None`) disables native composition for the
    /// focused editor. Disabling discards the window's queued
    /// [`ImeEvent::Update`]s, which belong to the old session; switch editors
    /// by disabling first. A request fails with [`Error::ImeUnavailable`]
    /// when [`Self::ime_available`] is false.
    fn configure_ime(&mut self, window: WindowId, request: Option<ImeRequest>) -> Result<()>;

    /// Offers `text` as the clipboard of `seat`, authorized by its latest input.
    fn set_clipboard(&mut self, window: WindowId, seat: SeatId, text: &str) -> Result<()>;

    /// Requests `seat`'s clipboard text for `window`; it arrives as
    /// [`Event::Clipboard`], possibly before the next dispatch. Empty or
    /// non-text selections produce no event.
    fn request_clipboard(&mut self, window: WindowId, seat: SeatId) -> Result<()>;

    /// Answers the latest [`Event::Drag`] of `seat` over `window`.
    fn accept_drag(&mut self, window: WindowId, seat: SeatId, accept: bool) -> Result<()>;

    /// Starts a copy drag of `data` from `window`, authorized by `seat`'s
    /// pressed pointer. Some platforms run the drag modally and return when it ends.
    fn start_drag(&mut self, window: WindowId, seat: SeatId, data: &DragData) -> Result<()>;

    /// Draws a software frame after [`Event::Redraw`].
    ///
    /// `draw` receives a tightly packed, top-down, premultiplied sRGB BGRA8
    /// buffer of the window's [`WindowInfo::buffer`] extent and must overwrite
    /// every rectangle of the region it is given; the rest holds the previous
    /// frame. `damage` is the area changed since the last presented frame in
    /// buffer pixels (`None`: everything). The surface is opaque: a translucent
    /// pixel shows its premultiplied color as if over black. Returns false
    /// without calling `draw` while the window cannot present yet. A drawing
    /// error ([`Error::Draw`]) never presents partial pixels.
    fn present(
        &mut self,
        window: WindowId,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool>;
}

/// A platform whose windows lend native handles to GPU presenters and
/// accessibility adapters.
pub trait SurfacePlatform: Platform {
    /// An owned lease on a window's native handles, implementing the
    /// `raw-window-handle` traits. The window stays alive while a lease
    /// exists; drop presenters before the platform.
    type Surface: 'static;

    /// Leases a window's native handles.
    fn window_surface(&self, window: WindowId) -> Result<Self::Surface>;

    /// Presents through an external graphics API after [`Event::Redraw`],
    /// using native frame pacing. `present` receives the buffer extent and
    /// returns whether it submitted a frame to the leased surface. Returns
    /// false without calling it while the window cannot present; errors and
    /// false results keep the redraw request for a retry.
    fn present_external(
        &mut self,
        window: WindowId,
        present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool>;
}
