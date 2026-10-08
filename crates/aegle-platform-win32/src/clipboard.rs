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
pub(crate) const CF_UNICODETEXT: u16 = 13;

/// UTF-16 units as native-endian bytes.
pub(crate) fn unit_bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_ne_bytes()).collect()
}

/// A new movable global block holding `bytes`, owned by the caller.
pub(crate) fn global(bytes: Vec<u8>) -> Result<HGLOBAL, Error> {
    // SAFETY: the fresh block is at least `bytes.len()` long and locked
    // while it is written; it is freed if it cannot be locked.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let target = GlobalLock(memory).cast::<u8>();
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(Error::Backend("GlobalLock failed".into()));
        }
        target.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
        let _ = GlobalUnlock(memory);
        Ok(memory)
    }
}

/// UTF-16 text in `memory` up to its first NUL; unpaired surrogates become
/// U+FFFD.
///
/// # Safety
/// `memory` must be a valid global block for the duration of the call.
pub(crate) unsafe fn read_text(memory: HGLOBAL) -> Option<String> {
    // SAFETY: reads stay within the locked block's size in UTF-16 units.
    unsafe {
        let source = GlobalLock(memory).cast::<u16>().cast_const();
        if source.is_null() {
            return None;
        }
        let units = std::slice::from_raw_parts(source, GlobalSize(memory) / 2);
        let end = units
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..end]);
        let _ = GlobalUnlock(memory);
        Some(text)
    }
}

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
        // SAFETY: the clipboard is open; ownership of the block passes to the
        // system only when SetClipboardData succeeds, otherwise it is freed here.
        unsafe {
            EmptyClipboard()?;
            let memory = global(unit_bytes(&units))?;
            if let Err(error) = SetClipboardData(u32::from(CF_UNICODETEXT), Some(HANDLE(memory.0)))
            {
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
        // until close.
        unsafe {
            let Ok(data) = GetClipboardData(u32::from(CF_UNICODETEXT)) else {
                return Ok(None);
            };
            Ok(read_text(HGLOBAL(data.0)))
        }
    }
}
