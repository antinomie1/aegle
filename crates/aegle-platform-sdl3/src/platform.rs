//! The platform contract on one SDL3 video connection.
#![allow(unsafe_code)]

use std::time::Duration;

use aegle_platform::{
    Cursor, DragData, DrawResult, Event, ImeRequest, PixelRect, PixelSize, Platform, Point,
    Preferences, Region, Result, SeatId, WakeHandle, WindowId, WindowInfo, WindowMode,
    WindowOptions,
};

use crate::Sdl3;

impl Platform for Sdl3 {
    fn connect() -> Result<Self> {
        Sdl3::connect()
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

    fn outputs(&self) -> Result<Vec<aegle_platform::OutputInfo>> {
        self.outputs()
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

    fn set_title(&mut self, window: WindowId, title: &str) -> Result<()> {
        self.set_title(window, title)
    }

    fn set_cursor(&mut self, window: WindowId, cursor: Cursor) -> Result<()> {
        self.set_cursor(window, cursor)
    }

    fn set_size(&mut self, window: WindowId, size: PixelSize) -> Result<()> {
        self.set_size(window, size)
    }

    fn set_size_limits(
        &mut self,
        window: WindowId,
        min: Option<PixelSize>,
        max: Option<PixelSize>,
    ) -> Result<()> {
        self.set_size_limits(window, min, max)
    }

    fn set_resizable(&mut self, window: WindowId, resizable: bool) -> Result<()> {
        self.set_resizable(window, resizable)
    }

    fn set_mode(&mut self, window: WindowId, mode: WindowMode) -> Result<()> {
        self.set_mode(window, mode)
    }

    fn position(&self, window: WindowId) -> Result<Point> {
        self.position(window)
    }

    fn set_position(&mut self, window: WindowId, position: Point) -> Result<()> {
        self.set_position(window, position)
    }

    fn set_always_on_top(&mut self, window: WindowId, on_top: bool) -> Result<()> {
        self.set_always_on_top(window, on_top)
    }

    fn set_decorations(&mut self, window: WindowId, decorated: bool) -> Result<()> {
        self.set_decorations(window, decorated)
    }

    fn set_areas(
        &mut self,
        window: WindowId,
        areas: &[(aegle_platform::Rect, aegle_platform::WindowArea)],
    ) -> Result<()> {
        self.set_areas(window, areas)
    }

    fn set_icon(&mut self, window: WindowId, icon: aegle_platform::Icon<'_>) -> Result<()> {
        self.set_icon(window, icon)
    }

    fn configure_ime(&mut self, window: WindowId, request: Option<ImeRequest>) -> Result<()> {
        self.configure_ime(window, request)
    }

    fn set_clipboard(&mut self, window: WindowId, seat: SeatId, text: &str) -> Result<()> {
        self.set_clipboard(window, seat, text)
    }

    fn request_clipboard(&mut self, window: WindowId, seat: SeatId) -> Result<()> {
        self.request_clipboard(window, seat)
    }

    fn accept_drag(&mut self, window: WindowId, seat: SeatId, accept: bool) -> Result<()> {
        self.accept_drag(window, seat, accept)
    }

    fn start_drag(&mut self, window: WindowId, seat: SeatId, data: &DragData) -> Result<()> {
        self.start_drag(window, seat, data)
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
impl aegle_platform::SurfacePlatform for Sdl3 {
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
