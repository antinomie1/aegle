//! Synchronous `CF_UNICODETEXT` clipboard access through a window's HWND.
#![allow(unsafe_code)]

use crate::{Error, Win32, WindowId};
use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
    },
};

/// Standard UTF-16 text clipboard format.
const CF_UNICODETEXT: u32 = 13;

/// Holds the process-wide clipboard open; dropping it closes the clipboard.
struct Open;

impl Open {
    fn new(hwnd: HWND) -> Result<Self, Error> {
        // SAFETY: the HWND is a live window owned by this backend's thread.
        unsafe { OpenClipboard(Some(hwnd)) }?;
        Ok(Self)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after OpenClipboard succeeded.
        let _ = unsafe { CloseClipboard() };
    }
}

impl Win32 {
    /// Replaces the system clipboard with UTF-16 text owned by `id`'s window.
    pub fn set_clipboard(&mut self, id: WindowId, text: &str) -> Result<(), Error> {
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let _open = Open::new(self.window(id)?.hwnd.get())?;
        // SAFETY: the clipboard is open. The fresh movable block is large enough
        // for `units`; ownership passes to the system only when SetClipboardData
        // succeeds, otherwise it is freed here.
        unsafe {
            EmptyClipboard()?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2)?;
            let target = GlobalLock(memory).cast::<u16>();
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(Error::Backend("GlobalLock failed".into()));
            }
            target.copy_from_nonoverlapping(units.as_ptr(), units.len());
            let _ = GlobalUnlock(memory);
            if let Err(error) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// Reads clipboard text up to its first NUL, or `None` without text.
    /// Unpaired surrogates become U+FFFD.
    pub fn clipboard_text(&mut self, id: WindowId) -> Result<Option<String>, Error> {
        let _open = Open::new(self.window(id)?.hwnd.get())?;
        // SAFETY: the clipboard is open, so the system-owned block stays valid
        // until close. Reads stay within its locked size in UTF-16 units.
        unsafe {
            let Ok(data) = GetClipboardData(CF_UNICODETEXT) else {
                return Ok(None);
            };
            let memory = HGLOBAL(data.0);
            let source = GlobalLock(memory).cast::<u16>().cast_const();
            if source.is_null() {
                return Ok(None);
            }
            let units = std::slice::from_raw_parts(source, GlobalSize(memory) / 2);
            let end = units
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..end]);
            let _ = GlobalUnlock(memory);
            Ok(Some(text))
        }
    }
}
