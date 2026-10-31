//! The platform of a Linux build that selected no window backend.
//!
//! `aegle-app`'s `native` feature asks for the native host; the backend crate
//! comes from `sdl3` or `wayland`, which are exclusive. A dependent crate
//! cannot choose one for its consumers, so this build still provides [`App`]
//! and fails when it connects, naming the missing feature — the same shape as a
//! build without a renderer. The `aegle` facade rejects this combination at
//! compile time with the rebuild command, so only direct `aegle-app` users see
//! this error.

use std::time::Duration;

use aegle_platform::{
    Cursor, DragData, DrawResult, Error, Event, ImeRequest, PixelRect, PixelSize, Platform,
    Preferences, Region, Result, SeatId, WakeHandle, WindowId, WindowInfo, WindowOptions,
};

/// What a build without a window backend reports.
const MESSAGE: &str = "this Linux build has no window backend; enable the `sdl3` feature \
                       (the default in `aegle`) or `wayland`";

/// An unreachable platform: [`Platform::connect`] always fails.
pub(crate) struct Missing;

impl Platform for Missing {
    fn connect() -> Result<Self> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn preferences(&self) -> Preferences {
        Preferences::default()
    }

    fn wake_handle(&mut self) -> Result<WakeHandle> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn ime_available(&self) -> bool {
        false
    }

    fn create_window(&mut self, _options: WindowOptions<'_>) -> Result<WindowId> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn remove_window(&mut self, _window: WindowId) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn window_info(&self, _window: WindowId) -> Result<WindowInfo> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn dispatch(&mut self, _timeout: Option<Duration>) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn next_event(&mut self) -> Option<Event> {
        None
    }

    fn request_redraw(&mut self, _window: WindowId) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn set_title(&mut self, _window: WindowId, _title: &str) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn set_cursor(&mut self, _window: WindowId, _cursor: Cursor) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn configure_ime(&mut self, _window: WindowId, _request: Option<ImeRequest>) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn set_clipboard(&mut self, _window: WindowId, _seat: SeatId, _text: &str) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn request_clipboard(&mut self, _window: WindowId, _seat: SeatId) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn accept_drag(&mut self, _window: WindowId, _seat: SeatId, _accept: bool) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn start_drag(&mut self, _window: WindowId, _seat: SeatId, _data: &DragData) -> Result<()> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn present(
        &mut self,
        _window: WindowId,
        _damage: Option<Region<PixelRect>>,
        _draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool> {
        Err(Error::Backend(MESSAGE.into()))
    }
}

/// A GPU lease of a build that cannot open a window.
#[cfg(any(feature = "vulkan", feature = "wgpu"))]
pub(crate) struct MissingSurface;

#[cfg(any(feature = "vulkan", feature = "wgpu"))]
impl raw_window_handle::HasWindowHandle for MissingSurface {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        Err(raw_window_handle::HandleError::Unavailable)
    }
}

#[cfg(any(feature = "vulkan", feature = "wgpu"))]
impl raw_window_handle::HasDisplayHandle for MissingSurface {
    fn display_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        Err(raw_window_handle::HandleError::Unavailable)
    }
}

#[cfg(any(feature = "vulkan", feature = "wgpu"))]
impl aegle_platform::SurfacePlatform for Missing {
    type Surface = MissingSurface;

    fn window_surface(&self, _window: WindowId) -> Result<Self::Surface> {
        Err(Error::Backend(MESSAGE.into()))
    }

    fn present_external(
        &mut self,
        _window: WindowId,
        _present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool> {
        Err(Error::Backend(MESSAGE.into()))
    }
}
