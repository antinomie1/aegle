//! Window management: content size and its limits, resizability, modes,
//! position and stacking of the toplevel windows.
//!
//! Limits are answered in `WM_GETMINMAXINFO`. Maximizing and minimizing use
//! `ShowWindow`; full screen is the borderless window covering its monitor,
//! with the style and placement it had saved to return to. A window is
//! hidden until its first frame, so a mode requested before then becomes the
//! command that first shows it. Desktop positions are physical pixels, read
//! and written with the window's current scale.
#![allow(unsafe_code)]

use crate::{Win32, native::Native};
use aegle_platform::{Error, PixelSize, Point, Result, WindowId, WindowMode, check_size_limits};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    },
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

/// A full-screen window's style and placement before it covered the monitor.
#[derive(Clone, Copy)]
pub(crate) struct Restore {
    style: isize,
    placement: WINDOWPLACEMENT,
}

fn backend(error: windows::core::Error) -> Error {
    Error::backend(error)
}

/// The physical extent of `size` at `scale`, rejecting what a window cannot take.
fn physical(size: PixelSize, scale: f32) -> Result<PixelSize> {
    let extent = |value: u32| (f64::from(value) * f64::from(scale)).ceil();
    let (width, height) = (extent(size.width), extent(size.height));
    if size.is_empty() || width > f64::from(i32::MAX / 4) || height > f64::from(i32::MAX / 4) {
        return Err(Error::InvalidSize);
    }
    Ok(PixelSize {
        width: width as u32,
        height: height as u32,
    })
}

impl Native {
    /// The mode the window shows in now; a hidden window is normal.
    pub(crate) fn mode(&self) -> WindowMode {
        let hwnd = self.hwnd.get();
        // SAFETY: state queries of this live window.
        unsafe {
            if self.fullscreen.get().is_some() {
                WindowMode::Fullscreen
            } else if IsIconic(hwnd).as_bool() {
                WindowMode::Minimized
            } else if IsZoomed(hwnd).as_bool() {
                WindowMode::Maximized
            } else {
                WindowMode::Normal
            }
        }
    }

    /// Reports a mode reached without a size change.
    fn report_mode(&self) {
        let info = self.info.get();
        if info.mode != self.mode() {
            self.configure(info.buffer, info.scale, info.active, info.configured);
        }
    }

    /// The outer window extent whose client area is `client` physical pixels.
    fn frame(&self, client: PixelSize) -> windows::core::Result<(i32, i32)> {
        let rect = self.outer(RECT {
            left: 0,
            top: 0,
            right: client.width as i32,
            bottom: client.height as i32,
        })?;
        Ok((rect.right - rect.left, rect.bottom - rect.top))
    }

    /// Answers `WM_GETMINMAXINFO` with the content limits as frame extents;
    /// a full-screen window covers its monitor regardless.
    pub(crate) fn min_max_info(&self, info: &mut MINMAXINFO) {
        let (min, max) = self.limits.get();
        if self.fullscreen.get().is_some() {
            return;
        }
        // SAFETY: a DPI query of this live window.
        let scale = unsafe { GetDpiForWindow(self.hwnd.get()) } as f32 / 96.0;
        let frame = |size: PixelSize| physical(size, scale).ok().and_then(|p| self.frame(p).ok());
        if let Some((x, y)) = min.and_then(frame) {
            info.ptMinTrackSize = POINT { x, y };
        }
        if let Some((x, y)) = max.and_then(frame) {
            info.ptMaxTrackSize = POINT { x, y };
        }
    }

    fn resize(&self, size: PixelSize) -> Result<()> {
        let (min, max) = self.limits.get();
        let client = physical(size.clamp(min, max), self.info.get().scale)?;
        let (width, height) = self.frame(client).map_err(backend)?;
        let flags = SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE;
        // SAFETY: repositioning this live window.
        unsafe { SetWindowPos(self.hwnd.get(), None, 0, 0, width, height, flags) }.map_err(backend)
    }

    /// Shows a window for its first frame in the mode requested meanwhile.
    pub(crate) fn show(&self) {
        let hwnd = self.hwnd.get();
        // SAFETY: showing this live window.
        unsafe {
            if IsWindowVisible(hwnd).as_bool() {
                return;
            }
            let command = match self.pending_mode.get() {
                WindowMode::Maximized => SW_SHOWMAXIMIZED,
                WindowMode::Minimized => SW_SHOWMINIMIZED,
                _ => SW_SHOWNORMAL,
            };
            let _ = ShowWindow(hwnd, command);
        }
        if self.pending_mode.get() == WindowMode::Fullscreen {
            let _ = self.enter_full_screen();
        }
        self.report_mode();
    }

    fn enter_full_screen(&self) -> Result<()> {
        let hwnd = self.hwnd.get();
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: queries and style changes of this live window; the
        // structures are writable and sized.
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            GetWindowPlacement(hwnd, &mut placement).map_err(backend)?;
            let screen = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if !GetMonitorInfoW(screen, &mut monitor).as_bool() {
                return Err(Error::backend("the window's monitor has no geometry"));
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            self.fullscreen.set(Some(Restore { style, placement }));
            SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_OVERLAPPEDWINDOW.0 as isize));
            let area = monitor.rcMonitor;
            SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                area.left,
                area.top,
                area.right - area.left,
                area.bottom - area.top,
                SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
            )
            .map_err(backend)
        }
    }

    fn leave_full_screen(&self) -> Result<()> {
        let Some(restore) = self.fullscreen.take() else {
            return Ok(());
        };
        let hwnd = self.hwnd.get();
        // SAFETY: restoring this live window's saved style and placement.
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, restore.style);
            SetWindowPlacement(hwnd, &restore.placement).map_err(backend)?;
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER;
            SetWindowPos(hwnd, None, 0, 0, 0, 0, flags | SWP_FRAMECHANGED).map_err(backend)
        }
    }

    fn set_mode(&self, target: WindowMode) -> Result<()> {
        let hwnd = self.hwnd.get();
        // SAFETY: a visibility query of this live window.
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            self.pending_mode.set(target);
            return Ok(());
        }
        let current = self.mode();
        if current == target {
            return Ok(());
        }
        if current == WindowMode::Fullscreen {
            self.leave_full_screen()?;
        }
        let command = match target {
            WindowMode::Fullscreen => return self.enter_full_screen().map(|()| self.report_mode()),
            WindowMode::Maximized => SW_MAXIMIZE,
            WindowMode::Minimized => SW_MINIMIZE,
            _ => SW_RESTORE,
        };
        // SAFETY: changing this live window's show state; restoring a normal
        // window has no effect.
        unsafe {
            let _ = ShowWindow(hwnd, command);
        }
        self.report_mode();
        Ok(())
    }
}

impl Win32 {
    fn normal(&self, id: WindowId) -> Result<&std::rc::Rc<Native>> {
        let native = self.window(id)?;
        if native.mode() != WindowMode::Normal {
            return Err(Error::InvalidRole(
                "restore the window before moving or resizing it",
            ));
        }
        Ok(native)
    }

    pub(crate) fn set_size(&mut self, id: WindowId, size: PixelSize) -> Result<()> {
        self.normal(id)?.resize(size)
    }

    pub(crate) fn set_size_limits(
        &mut self,
        id: WindowId,
        min: Option<PixelSize>,
        max: Option<PixelSize>,
    ) -> Result<()> {
        check_size_limits(min, max)?;
        let native = self.window(id)?;
        if let Some(min) = min {
            physical(min, native.info.get().scale)?;
        }
        native.limits.set((min, max));
        let size = native.info.get().size;
        if native.mode() == WindowMode::Normal && size.clamp(min, max) != size {
            native.resize(size)?;
        }
        Ok(())
    }

    pub(crate) fn set_resizable(&mut self, id: WindowId, resizable: bool) -> Result<()> {
        let native = self.window(id)?;
        let edge = (WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0) as isize;
        let toggle = |style: isize| match resizable {
            true => style | edge,
            false => style & !edge,
        };
        // A full-screen window takes it when it returns.
        if let Some(mut restore) = native.fullscreen.get() {
            restore.style = toggle(restore.style);
            native.fullscreen.set(Some(restore));
            return Ok(());
        }
        let hwnd = native.hwnd.get();
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
        // SAFETY: restyling this live window and refreshing its frame.
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, toggle(GetWindowLongPtrW(hwnd, GWL_STYLE)));
            SetWindowPos(hwnd, None, 0, 0, 0, 0, flags).map_err(backend)
        }
    }

    pub(crate) fn set_mode(&mut self, id: WindowId, mode: WindowMode) -> Result<()> {
        self.window(id)?.set_mode(mode)
    }

    /// The client area's top-left corner on the desktop, in physical pixels
    /// at the window's scale, and the window's offset from it.
    fn client_origin(native: &Native) -> Result<(POINT, RECT)> {
        let hwnd = native.hwnd.get();
        let mut origin = POINT::default();
        let mut frame = RECT::default();
        // SAFETY: geometry queries of this live window into writable values.
        unsafe {
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return Err(Error::backend("the window has no client area"));
            }
            GetWindowRect(hwnd, &mut frame).map_err(backend)?;
        }
        Ok((origin, frame))
    }

    pub(crate) fn position(&self, id: WindowId) -> Result<Point> {
        let native = self.window(id)?;
        let (origin, _) = Self::client_origin(native)?;
        let scale = native.info.get().scale;
        Ok(Point::new(origin.x as f32 / scale, origin.y as f32 / scale))
    }

    pub(crate) fn set_position(&mut self, id: WindowId, position: Point) -> Result<()> {
        let native = self.normal(id)?;
        let scale = native.info.get().scale;
        let to = |value: f32| {
            let pixels = (f64::from(value) * f64::from(scale)).round();
            (pixels.is_finite() && pixels.abs() <= f64::from(i32::MAX / 4)).then_some(pixels as i32)
        };
        let (Some(x), Some(y)) = (to(position.x), to(position.y)) else {
            return Err(Error::InvalidPosition);
        };
        let (origin, frame) = Self::client_origin(native)?;
        let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        // SAFETY: moving this live window by its frame's offset from the client.
        unsafe {
            SetWindowPos(
                native.hwnd.get(),
                None,
                x - (origin.x - frame.left),
                y - (origin.y - frame.top),
                0,
                0,
                flags,
            )
        }
        .map_err(backend)
    }

    pub(crate) fn set_always_on_top(&mut self, id: WindowId, on_top: bool) -> Result<()> {
        let hwnd: HWND = self.window(id)?.hwnd.get();
        let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        // SAFETY: changing this live window's z-order band.
        unsafe { SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, flags) }.map_err(backend)
    }
}
