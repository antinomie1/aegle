//! Undecorated windows, window areas and the window icon.
//!
//! An undecorated window keeps its overlapped style, so the system still
//! snaps, animates and shadows it, and answers `WM_NCCALCSIZE` with a client
//! area covering the whole window. `WM_NCHITTEST` then names the parts that
//! move or resize it: the resize border of an undecorated window first, then
//! the host's areas, so the system runs the move or resize itself with its
//! own cursors, snapping and double click.
#![allow(unsafe_code)]

use crate::{Win32, native::Native};
use aegle_platform::{
    Error, Icon, PixelSize, Point, Rect, ResizeEdge, Result, WindowArea, WindowId, WindowMode,
    area_at, border_edge, check_areas,
};
use windows::Win32::{
    Foundation::{LPARAM, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, ScreenToClient,
    },
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
        WindowsAndMessaging::*,
    },
};

fn backend(error: windows::core::Error) -> Error {
    Error::backend(error)
}

/// The hit-test code that resizes by `edge`.
fn hit_code(edge: ResizeEdge) -> u32 {
    match edge {
        ResizeEdge::Top => HTTOP,
        ResizeEdge::Bottom => HTBOTTOM,
        ResizeEdge::Left => HTLEFT,
        ResizeEdge::Right => HTRIGHT,
        ResizeEdge::TopLeft => HTTOPLEFT,
        ResizeEdge::TopRight => HTTOPRIGHT,
        ResizeEdge::BottomLeft => HTBOTTOMLEFT,
        ResizeEdge::BottomRight => HTBOTTOMRIGHT,
    }
}

impl Native {
    /// The window rectangle whose client area is `client`, in the same
    /// coordinates: `client` itself for an undecorated window.
    pub(crate) fn outer(&self, mut client: RECT) -> windows::core::Result<RECT> {
        if self.decorated.get() {
            let hwnd = self.hwnd.get();
            // SAFETY: style and DPI queries of this live window; `client` is writable.
            unsafe {
                let style = WINDOW_STYLE(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32);
                let extended = WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32);
                let dpi = GetDpiForWindow(hwnd);
                AdjustWindowRectExForDpi(&mut client, style, false, extended, dpi)?;
            }
        }
        Ok(client)
    }

    /// Answers `WM_NCCALCSIZE` for an undecorated window: the client area is
    /// the proposed window rectangle in `rect`, except that a maximized
    /// window, whose frame the system hangs over the monitor's edges, takes
    /// the monitor's work area. Returns false for a window that keeps the
    /// system's frame.
    pub(crate) fn client_area(&self, rect: &mut RECT) -> bool {
        if self.decorated.get() || self.fullscreen.get().is_some() {
            return false;
        }
        let hwnd = self.hwnd.get();
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: state and monitor queries of this live window into a sized
        // structure.
        unsafe {
            let screen = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if IsZoomed(hwnd).as_bool() && GetMonitorInfoW(screen, &mut monitor).as_bool() {
                *rect = monitor.rcWork;
            }
        }
        true
    }

    /// Answers `WM_NCHITTEST` at the screen position in `position` where the
    /// window's border or areas decide it; `None` leaves it to the system.
    pub(crate) fn hit_test(&self, position: LPARAM) -> Option<isize> {
        let decorated = self.decorated.get();
        if self.fullscreen.get().is_some() || (decorated && self.areas.borrow().is_empty()) {
            return None;
        }
        let hwnd = self.hwnd.get();
        if decorated {
            // The system's frame comes first; areas divide the client area.
            // SAFETY: the default answer for this live window and position.
            let system = unsafe { DefWindowProcW(hwnd, WM_NCHITTEST, WPARAM(0), position) };
            if system.0 != HTCLIENT as isize {
                return Some(system.0);
            }
        }
        let mut point = POINT {
            x: i32::from(position.0 as u16 as i16),
            y: i32::from((position.0 >> 16) as u16 as i16),
        };
        // SAFETY: converting a point for this live window in place; a style query.
        let resizable = unsafe {
            let _ = ScreenToClient(hwnd, &mut point);
            GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_THICKFRAME.0 != 0
        };
        let info = self.info.get();
        let at = Point::new(point.x as f32 / info.scale, point.y as f32 / info.scale);
        let resizes = resizable && self.mode() == WindowMode::Normal;
        let border = match !decorated && resizes {
            true => border_edge(info.size, at),
            false => None,
        };
        let code =
            match border.map_or_else(|| area_at(&self.areas.borrow(), at), WindowArea::Resize) {
                WindowArea::Move => HTCAPTION,
                WindowArea::Resize(edge) if resizes => hit_code(edge),
                _ => HTCLIENT,
            };
        Some(code as isize)
    }

    /// Recomputes the frame after the decorations changed, around the client
    /// area that was at `origin` on the screen; a window that is not in its
    /// normal mode keeps its rectangle.
    fn reframe(&self, origin: POINT, client: PixelSize) -> Result<()> {
        let hwnd = self.hwnd.get();
        let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
        if self.mode() != WindowMode::Normal {
            // SAFETY: refreshing this live window's frame in place.
            return unsafe {
                SetWindowPos(hwnd, None, 0, 0, 0, 0, flags | SWP_NOMOVE | SWP_NOSIZE)
            }
            .map_err(backend);
        }
        let outer = self
            .outer(RECT {
                left: origin.x,
                top: origin.y,
                right: origin.x + client.width as i32,
                bottom: origin.y + client.height as i32,
            })
            .map_err(backend)?;
        // SAFETY: moving this live window to the frame around its client area.
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                outer.left,
                outer.top,
                outer.right - outer.left,
                outer.bottom - outer.top,
                flags,
            )
        }
        .map_err(backend)
    }
}

impl Win32 {
    pub(crate) fn set_decorations(&mut self, id: WindowId, decorated: bool) -> Result<()> {
        let native = self.window(id)?;
        if native.decorated.get() == decorated {
            return Ok(());
        }
        let mut origin = POINT::default();
        // SAFETY: a geometry query of this live window into a writable value.
        if !unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(native.hwnd.get(), &mut origin) }
            .as_bool()
        {
            return Err(Error::backend("the window has no client area"));
        }
        let client = native.info.get().buffer;
        native.decorated.set(decorated);
        native.reframe(origin, client)
    }

    pub(crate) fn set_areas(&mut self, id: WindowId, areas: &[(Rect, WindowArea)]) -> Result<()> {
        check_areas(areas)?;
        let mut kept = self.window(id)?.areas.borrow_mut();
        kept.clear();
        kept.extend_from_slice(areas);
        Ok(())
    }

    /// Shows `icon` in the title bar, the task bar and the window switcher;
    /// the system scales the one image to each size.
    pub(crate) fn set_icon(&mut self, id: WindowId, icon: Icon<'_>) -> Result<()> {
        let native = self.window(id)?;
        let (Ok(width), Ok(height)) = (i32::try_from(icon.width), i32::try_from(icon.height))
        else {
            return Err(Error::InvalidSize);
        };
        if !icon.is_valid() {
            return Err(Error::InvalidSize);
        }
        // Straight alpha in the color bits decides coverage, so the
        // one-bit-per-pixel mask, rows padded to 16 bits, stays clear.
        let bgra: Vec<u8> = icon
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [b, g, r, a])
            .collect();
        let mask = vec![0u8; icon.width.div_ceil(16) as usize * 2 * icon.height as usize];
        // SAFETY: both arrays hold the bits the extents and depths name, and
        // the system copies them.
        let handle =
            unsafe { CreateIcon(None, width, height, 1, 32, mask.as_ptr(), bgra.as_ptr()) }
                .map_err(backend)?;
        let hwnd = native.hwnd.get();
        // SAFETY: this live window takes the icon for both sizes; the icon it
        // replaces is no longer shown and was created here.
        unsafe {
            for kind in [ICON_BIG, ICON_SMALL] {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(handle.0 as isize)),
                );
            }
            let old = native.icon.replace(handle);
            if !old.is_invalid() {
                let _ = DestroyIcon(old);
            }
        }
        Ok(())
    }
}
