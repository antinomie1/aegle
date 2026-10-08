//! The notification-area icon, its balloon and its popup menu.

use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
            DeleteObject,
        },
        UI::{
            Shell::{
                NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD,
                NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4,
                NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, HICON,
                HMENU, ICONINFO, IDI_APPLICATION, LoadIconW, MF_CHECKED, MF_GRAYED, MF_POPUP,
                MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_RETURNCMD,
                TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
            },
        },
    },
    core::PCWSTR,
};

use super::{OwnedTray, TRAY, Worker, wide, with};
use crate::{Event, MenuItem};

/// An owned menu entry.
pub(super) enum Entry {
    Item {
        id: u32,
        label: Vec<u16>,
        enabled: bool,
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: Vec<u16>,
        items: Vec<Entry>,
    },
}

pub(super) fn entries(items: &[MenuItem<'_>]) -> Vec<Entry> {
    items
        .iter()
        .map(|item| match *item {
            MenuItem::Item {
                id,
                label,
                enabled,
                checked,
            } => Entry::Item {
                id,
                label: wide(label),
                enabled,
                checked,
            },
            MenuItem::Separator => Entry::Separator,
            MenuItem::Submenu { label, items } => Entry::Submenu {
                label: wide(label),
                items: entries(items),
            },
        })
        .collect()
}

/// Copies `text` into a fixed UTF-16 field, truncated and NUL-terminated.
pub(super) fn copy(field: &mut [u16], text: &str) {
    let units = text.encode_utf16().take(field.len() - 1).chain([0]);
    field
        .iter_mut()
        .zip(units)
        .for_each(|(slot, unit)| *slot = unit);
}

/// An icon from top-down BGRA pixels.
pub(super) fn icon(width: u32, height: u32, bgra: &[u8]) -> windows::core::Result<HICON> {
    // SAFETY: the DIB section is `width * height * 4` bytes, which `bgra`
    // fills exactly; both bitmaps are deleted once the icon copied them.
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), bgra.len());
        let mask = CreateBitmap(width as i32, height as i32, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

impl Worker {
    pub(super) fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY,
            hIcon: self.icon.map(|(icon, _)| icon).unwrap_or_default(),
            ..Default::default()
        };
        copy(&mut data.szTip, &self.tooltip);
        data
    }

    /// Adds the icon to the notification area, or updates it.
    pub(super) fn show(&self, message: NOTIFY_ICON_MESSAGE) -> bool {
        let mut data = self.data();
        // SAFETY: `data` is a complete, sized NOTIFYICONDATAW.
        let shown = unsafe { Shell_NotifyIconW(message, &data) }.as_bool();
        if shown && message == NIM_ADD {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            // SAFETY: as above.
            let _ = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) };
        }
        shown
    }

    pub(super) fn remove(&mut self) {
        if let Some((icon, _)) = self.icon.take() {
            let data = self.data();
            // SAFETY: as in `show`; the icon is no longer referenced after this.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                let _ = DestroyIcon(icon);
            }
        }
    }
}

/// Shows the tray menu at `(x, y)` and reports the chosen item. The menu's
/// modal loop dispatches messages, so the worker is not borrowed meanwhile.
pub(super) fn popup(x: i32, y: i32) {
    let mut ids = Vec::new();
    // SAFETY: labels are copied into the menu, which is destroyed below.
    let built = with(|worker| {
        unsafe { build(&worker.menu, &mut ids) }
            .ok()
            .map(|menu| (menu, worker.hwnd, worker.sink.clone()))
    });
    let Some((menu, hwnd, sink)) = built.flatten() else {
        return;
    };
    // SAFETY: a menu built above for this thread's window.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            x,
            y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        if let Some(&item) = (chosen.0 as usize).checked_sub(1).and_then(|i| ids.get(i)) {
            sink(Event::TrayMenu { item });
        }
    }
}

/// A popup menu of `entries`; command `n` reports `ids[n - 1]`.
unsafe fn build(entries: &[Entry], ids: &mut Vec<u32>) -> windows::core::Result<HMENU> {
    // SAFETY: the caller destroys the returned menu, which owns submenus.
    unsafe {
        let menu = CreatePopupMenu()?;
        for entry in entries {
            match entry {
                Entry::Item {
                    id,
                    label,
                    enabled,
                    checked,
                } => {
                    ids.push(*id);
                    let mut flags = MF_STRING;
                    if !enabled {
                        flags |= MF_GRAYED;
                    }
                    if *checked == Some(true) {
                        flags |= MF_CHECKED;
                    }
                    AppendMenuW(menu, flags, ids.len(), PCWSTR(label.as_ptr()))?;
                }
                Entry::Separator => AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())?,
                Entry::Submenu { label, items } => {
                    let submenu = build(items, ids)?;
                    AppendMenuW(menu, MF_POPUP, submenu.0 as usize, PCWSTR(label.as_ptr()))?;
                }
            }
        }
        Ok(menu)
    }
}

impl Worker {
    /// Shows or updates the icon with `tray`.
    pub(super) fn set(&mut self, tray: OwnedTray) {
        let OwnedTray {
            width,
            height,
            bgra,
            tooltip,
            menu,
        } = tray;
        let handle = match icon(width, height, &bgra) {
            Ok(handle) => handle,
            Err(error) => {
                return self.emit(Event::Unavailable(format!("tray: {}", error.message())));
            }
        };
        let added = self.icon.is_none();
        if let Some((old, _)) = self.icon.replace((handle, true)) {
            // SAFETY: the old icon is replaced below and not used again.
            let _ = unsafe { DestroyIcon(old) };
        }
        (self.tooltip, self.menu) = (tooltip, menu);
        if !self.show(if added { NIM_ADD } else { NIM_MODIFY }) {
            self.emit(Event::Unavailable("tray: no notification area".into()));
        }
    }

    /// Shows a notification as the icon's balloon, adding a default icon
    /// when no tray is shown.
    pub(super) fn balloon(&mut self, id: u32, summary: &str, body: &str) {
        if self.icon.is_none() {
            // SAFETY: a shared system icon, never destroyed by us.
            let Ok(default) = (unsafe { LoadIconW(None, IDI_APPLICATION) }) else {
                return;
            };
            self.icon = Some((default, false));
            self.show(NIM_ADD);
        }
        if let Some(previous) = self.balloon.replace(id) {
            self.emit(Event::NotificationClosed {
                notification: previous,
            });
        }
        let mut data = self.data();
        data.uFlags |= NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        copy(&mut data.szInfoTitle, summary);
        copy(&mut data.szInfo, body);
        // SAFETY: as in `show`.
        if !unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool() {
            self.balloon = None;
            self.emit(Event::Unavailable(
                "notification: no notification area".into(),
            ));
        }
    }
}
