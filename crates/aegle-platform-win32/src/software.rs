#![allow(unsafe_code)]
use crate::{Error, PixelSize, PresentError, Win32, WindowId};
use aegle_types::PixelRect;
use windows::Win32::{
    Graphics::{Dwm::DwmFlush, Gdi::*},
    UI::WindowsAndMessaging::*,
};

impl Win32 {
    /// Borrows a tightly packed, top-down premultiplied-sRGB RGBA8 framebuffer.
    /// `damage` is the area changed since the last frame (`None`: all of it);
    /// `draw` must overwrite the region it is given, which is the whole buffer
    /// after a resize or failed frame, with an opaque background (alpha=255).
    /// Drawing errors retain the last displayed frame. One CPU buffer is reused;
    /// GDI upload and compositor-owned storage are outside `buffer_budget`.
    /// Software frames wait for DWM completion, avoiding a busy animation loop.
    pub fn present<E>(
        &mut self,
        id: WindowId,
        damage: Option<PixelRect>,
        draw: impl FnOnce(&mut [u8], PixelSize, PixelRect) -> Result<(), E>,
    ) -> Result<bool, PresentError<E>> {
        let native = self.window(id).map_err(PresentError::Platform)?;
        native.redraw_queued.set(false);
        if !native.info.get().configured {
            return Ok(false);
        }
        let size = native
            .info
            .get()
            .buffer_size()
            .map_err(PresentError::Platform)?;
        let length = size.bytes().map_err(PresentError::Platform)?;
        let mut pixels = native.pixels.borrow_mut();
        if length > native.budget {
            return Err(PresentError::Platform(Error::BufferBudget {
                required: length,
                budget: native.budget,
            }));
        }
        if pixels.capacity() < length {
            // Free an obsolete allocation first; resize never temporarily retains
            // two full framebuffers against a single-buffer budget.
            *pixels = Vec::new();
            pixels
                .try_reserve_exact(length)
                .map_err(|e| PresentError::Platform(Error::Backend(e.to_string())))?;
        }
        pixels.resize(length, 0);
        let full = PixelRect::full(size.width, size.height);
        let region = match native.drawn.take() {
            Some(drawn) if drawn == size => damage.unwrap_or(full),
            _ => full,
        };
        draw(&mut pixels, size, region).map_err(PresentError::Draw)?;
        let columns = region.x as usize * 4..(region.x + region.width) as usize * 4;
        let rows = || {
            pixels
                .chunks_exact(size.width as usize * 4)
                .skip(region.y as usize)
                .take(region.height as usize)
        };
        if rows().any(|row| row[columns.clone()].chunks_exact(4).any(|p| p[3] != 255)) {
            return Err(PresentError::Platform(Error::UnsupportedTransparency));
        }
        for row in pixels
            .chunks_exact_mut(size.width as usize * 4)
            .skip(region.y as usize)
            .take(region.height as usize)
        {
            for pixel in row[columns.clone()].chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.width as i32,
                biHeight: -(size.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: owned HWND, exact validated pixel extent with BGRA rows, valid
        // BITMAPINFO. GDI copies bytes synchronously; the DC is always released.
        unsafe {
            let dc = GetDC(Some(native.hwnd.get()));
            if dc.is_invalid() {
                return Err(PresentError::Platform(Error::Backend(
                    "GetDC failed".into(),
                )));
            }
            let rows = SetDIBitsToDevice(
                dc,
                0,
                0,
                size.width,
                size.height,
                0,
                0,
                0,
                size.height,
                pixels.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
            );
            ReleaseDC(Some(native.hwnd.get()), dc);
            if rows == 0 {
                return Err(PresentError::Platform(Error::Backend(
                    "GDI bitmap upload failed".into(),
                )));
            }
            if !IsWindowVisible(native.hwnd.get()).as_bool() {
                let _ = ShowWindow(native.hwnd.get(), SW_SHOWNORMAL);
            }
            DwmFlush().map_err(|e| PresentError::Platform(e.into()))?;
        }
        native.dirty.set(false);
        native.drawn.set(Some(size));
        Ok(true)
    }
}
