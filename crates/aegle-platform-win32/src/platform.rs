//! The platform contract on the Win32 message loop. Windows has one seat,
//! [`crate::SEAT`], and no layer surfaces.

use std::time::Duration;

use aegle_platform::{
    Cursor, DragData, DrawResult, Error, Event, ImeRequest, PixelRect, Platform, Preferences,
    Region, Result, SeatId, SurfacePlatform, WakeHandle,
};

use crate::{PixelSize, Win32, WindowId, WindowInfo, WindowOptions, WindowSurface};

impl Platform for Win32 {
    fn connect() -> Result<Self> {
        Ok(Win32::connect()?)
    }

    fn preferences(&self) -> Preferences {
        self.preferences()
    }

    fn wake_handle(&mut self) -> Result<WakeHandle> {
        Ok(self.wake_handle()?)
    }

    fn ime_available(&self) -> bool {
        self.ime_available()
    }

    fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId> {
        if options.layer.is_some() {
            return Err(Error::Unsupported("layer surfaces"));
        }
        Ok(self.create_window(options)?)
    }

    fn remove_window(&mut self, window: WindowId) -> Result<()> {
        Ok(self.remove_window(window)?)
    }

    fn window_info(&self, window: WindowId) -> Result<WindowInfo> {
        Ok(self.window_info(window)?)
    }

    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.dispatch(timeout)?)
    }

    fn next_event(&mut self) -> Option<Event> {
        self.next_event()
    }

    fn request_redraw(&mut self, window: WindowId) -> Result<()> {
        Ok(self.request_redraw(window)?)
    }

    fn set_title(&mut self, window: WindowId, title: &str) -> Result<()> {
        Ok(self.set_title(window, title)?)
    }

    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()> {
        Ok(self.set_cursor(window, cursor)?)
    }

    fn configure_ime(&mut self, window: WindowId, request: Option<ImeRequest>) -> Result<()> {
        Ok(self.configure_ime(window, request)?)
    }

    fn set_clipboard(&mut self, window: WindowId, _: SeatId, text: &str) -> Result<()> {
        Ok(self.set_clipboard(window, text)?)
    }

    fn request_clipboard(&mut self, window: WindowId, _: SeatId) -> Result<()> {
        Ok(self.request_clipboard(window)?)
    }

    fn accept_drag(&mut self, window: WindowId, _: SeatId, accept: bool) -> Result<()> {
        Ok(self.accept_drag(window, accept)?)
    }

    fn start_drag(&mut self, window: WindowId, _: SeatId, data: &DragData) -> Result<()> {
        Ok(self.start_drag(window, data)?)
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

impl SurfacePlatform for Win32 {
    type Surface = WindowSurface;

    fn window_surface(&self, window: WindowId) -> Result<WindowSurface> {
        Ok(self.window_surface(window)?)
    }

    fn present_external(
        &mut self,
        window: WindowId,
        present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool> {
        self.present_external(window, present)
    }
}
