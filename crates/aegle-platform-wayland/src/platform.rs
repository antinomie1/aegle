//! The platform contract on a Wayland connection. Seats are looked up by
//! their contract identity; a seat that has disappeared ignores requests.

use std::time::Duration;

use aegle_platform::{
    Cursor, DragData, DrawResult, Event, ImeRequest, PixelRect, Platform, Preferences, Region,
    Result, SeatId, WakeHandle,
};

use crate::{PixelSize, Wayland, WindowId, WindowInfo, WindowOptions};

impl Platform for Wayland {
    fn connect() -> Result<Self> {
        Wayland::connect()
    }

    fn preferences(&self) -> Preferences {
        self.preferences()
    }

    fn wake_handle(&mut self) -> Result<WakeHandle> {
        self.wake_handle()
    }

    fn ime_available(&self) -> bool {
        self.ime_available()
    }

    fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId> {
        self.create_window(options)
    }

    fn remove_window(&mut self, window: WindowId) -> Result<()> {
        self.remove_window(window)
    }

    fn window_info(&self, window: WindowId) -> Result<WindowInfo> {
        self.window_info(window)
    }

    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()> {
        self.dispatch(timeout)
    }

    fn next_event(&mut self) -> Option<Event> {
        self.next_event()
    }

    fn request_redraw(&mut self, window: WindowId) -> Result<()> {
        self.request_redraw(window)
    }

    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()> {
        self.set_cursor(window, cursor)
    }

    fn configure_ime(&mut self, window: WindowId, request: Option<ImeRequest>) -> Result<()> {
        self.configure_ime(window, request)
    }

    fn set_clipboard(&mut self, window: WindowId, seat: SeatId, text: &str) -> Result<()> {
        self.window_info(window)?;
        match self.state.seat(seat) {
            Some(seat) => self.set_clipboard(&seat, text),
            None => Ok(()),
        }
    }

    fn request_clipboard(&mut self, window: WindowId, seat: SeatId) -> Result<()> {
        self.window_info(window)?;
        match self.state.seat(seat) {
            Some(seat) => self.request_clipboard(window, &seat),
            None => Ok(()),
        }
    }

    fn accept_drag(&mut self, window: WindowId, seat: SeatId, accept: bool) -> Result<()> {
        self.window_info(window)?;
        if let Some(seat) = self.state.seat(seat) {
            self.accept_drag(&seat, accept);
        }
        Ok(())
    }

    fn start_drag(&mut self, window: WindowId, seat: SeatId, data: &DragData) -> Result<()> {
        match self.state.seat(seat) {
            Some(seat) => self.start_drag(window, &seat, data),
            None => self.window_info(window).map(drop),
        }
    }

    fn present(
        &mut self,
        window: WindowId,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool> {
        self.present(window, damage, draw)
    }
}

#[cfg(feature = "gpu")]
impl aegle_platform::SurfacePlatform for Wayland {
    type Surface = crate::WindowSurface;

    fn window_surface(&self, window: WindowId) -> Result<crate::WindowSurface> {
        self.window_surface(window)
    }

    fn present_external(
        &mut self,
        window: WindowId,
        present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool> {
        self.present_external(window, present)
    }
}
