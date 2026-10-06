//! Platform adapter construction, keeping native handles owned through teardown.
#![allow(unsafe_code)]

use crate::{native::Runtime, platform::WindowId};
use aegle_ui::Result;

#[cfg(target_os = "linux")]
pub(crate) type Adapter = aegle_access::UnixAdapter;
#[cfg(target_os = "windows")]
pub(crate) type Adapter = aegle_access::WindowsAdapter<crate::platform::WindowSurface>;

pub(crate) fn create(runtime: &mut Runtime, id: WindowId) -> Result<Adapter> {
    let wake = runtime.backend.wake_handle()?;
    #[cfg(target_os = "linux")]
    {
        let _ = id;
        Ok(Adapter::new(move || wake.wake()))
    }
    #[cfg(target_os = "windows")]
    {
        let window = runtime.backend.window_surface(id)?;
        // SAFETY: the owned lease keeps this HWND and its window-procedure context
        // alive until the adapter has removed its subclass during destruction.
        Ok(unsafe { Adapter::new(window, move || wake.wake())? })
    }
}
