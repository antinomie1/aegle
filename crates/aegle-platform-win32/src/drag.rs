//! OLE drag and drop: every window is an `IDropTarget` for text and files,
//! and [`Win32::start_drag`] runs a shell copy drag.
//!
//! OLE asks a target for its effect synchronously, while the host answers
//! [`Event::Drag`] later with [`Win32::accept_drag`]. Until the host has
//! answered for the current drag, a window accepts any text or files, so a
//! drag that this thread's own modal drag loop delivers still lands.
#![allow(unsafe_code)]

use std::{
    cell::Cell,
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    rc::{Rc, Weak},
};

use aegle_types::{DragData, Point};
use windows::{
    Win32::{
        Foundation::{GlobalFree, POINT, POINTL, S_OK},
        Graphics::Gdi::ScreenToClient,
        System::{
            Com::{
                DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
            },
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropSource, IDropTarget,
                IDropTarget_Impl, ReleaseStgMedium,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{DragQueryFileW, HDROP, SHCreateDataObject, SHDoDragDrop},
    },
    core::{Ref, implement},
};

use crate::{
    Error, Event, Win32, WindowId,
    clipboard::{CF_UNICODETEXT, global, read_text, unit_bytes},
    native::Native,
};

/// The shell's file list format.
const CF_HDROP: u16 = 15;

fn format(format: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// Whether `data` offers files or text.
fn supported(data: &IDataObject) -> bool {
    // SAFETY: plain queries on a live data object.
    [CF_HDROP, CF_UNICODETEXT]
        .into_iter()
        .any(|cf| unsafe { data.QueryGetData(&format(cf)) } == S_OK)
}

/// Reads files, or else text, from `data`.
fn read(data: &IDataObject) -> Option<DragData> {
    // SAFETY: each medium is released after use; DragQueryFileW writes at
    // most the buffer it is given.
    unsafe {
        if let Ok(mut medium) = data.GetData(&format(CF_HDROP)) {
            let drop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            let files = (0..count)
                .map(|index| {
                    let length = DragQueryFileW(drop, index, None) as usize;
                    let mut name = vec![0; length + 1];
                    DragQueryFileW(drop, index, Some(&mut name));
                    PathBuf::from(OsString::from_wide(&name[..length]))
                })
                .collect();
            ReleaseStgMedium(&mut medium);
            return Some(DragData::Files(files));
        }
        let mut medium = data.GetData(&format(CF_UNICODETEXT)).ok()?;
        let text = read_text(medium.u.hGlobal);
        ReleaseStgMedium(&mut medium);
        text.map(DragData::Text)
    }
}

/// `DROPFILES` followed by NUL-terminated wide paths and a final NUL.
fn file_list(files: &[PathBuf]) -> Vec<u8> {
    // pFiles (the header size), pt.x, pt.y, fNC, fWide.
    let mut bytes: Vec<u8> = [20u32, 0, 0, 0, 1]
        .iter()
        .flat_map(|field| field.to_ne_bytes())
        .collect();
    let mut units: Vec<u16> = files
        .iter()
        .flat_map(|file| file.as_os_str().encode_wide().chain([0]))
        .collect();
    units.push(0);
    bytes.extend(unit_bytes(&units));
    bytes
}

#[implement(IDropTarget)]
pub(crate) struct Target {
    native: Weak<Native>,
    /// Whether the current drag carries text or files.
    active: Cell<bool>,
    /// Last reported position, to report motion only.
    at: Cell<Option<Point>>,
}

/// A drop target reporting to `native`'s window.
pub(crate) fn target(native: &Rc<Native>) -> IDropTarget {
    Target {
        native: Rc::downgrade(native),
        active: Cell::new(false),
        at: Cell::new(None),
    }
    .into()
}

impl Target {
    fn native(&self) -> Option<Rc<Native>> {
        self.native.upgrade().filter(|n| n.registered.get())
    }

    /// The local logical position of screen point `point`.
    fn position(native: &Native, point: &POINTL) -> Point {
        let mut local = POINT {
            x: point.x,
            y: point.y,
        };
        // SAFETY: the window is live while registered.
        let _ = unsafe { ScreenToClient(native.hwnd.get(), &mut local) };
        let scale = native.info.get().scale;
        Point::new(local.x as f32 / scale, local.y as f32 / scale)
    }

    fn effect(&self, native: &Native) -> DROPEFFECT {
        if self.active.get() && native.drag_accept.get() != Some(false) {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        }
    }

    fn over(&self, point: &POINTL, effect: *mut DROPEFFECT) {
        let Some(native) = self.native() else {
            // SAFETY: OLE passes a valid effect pointer.
            unsafe { *effect = DROPEFFECT_NONE };
            return;
        };
        let position = Self::position(&native, point);
        if self.active.get() && self.at.replace(Some(position)) != Some(position) {
            native.emit(Event::Drag {
                window: native.id,
                position,
            });
        }
        // SAFETY: as above.
        unsafe { *effect = self.effect(&native) };
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        self.active.set(data.ok().is_ok_and(supported));
        self.at.set(None);
        if let Some(native) = self.native() {
            native.drag_accept.set(None);
        }
        self.over(point, effect);
        Ok(())
    }

    fn DragOver(
        &self,
        _: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        self.over(point, effect);
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        if self.active.replace(false)
            && let Some(native) = self.native()
        {
            native.emit(Event::DragLeave { window: native.id });
        }
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        // SAFETY: OLE passes a valid effect pointer.
        unsafe { *effect = DROPEFFECT_NONE };
        let Some(native) = self.native() else {
            return Ok(());
        };
        let accepted = self.effect(&native) == DROPEFFECT_COPY;
        self.active.set(false);
        let window = native.id;
        match accepted.then(|| data.ok().ok().and_then(read)).flatten() {
            Some(data) => {
                let position = Target::position(&native, point);
                native.emit(Event::Drop {
                    window,
                    position,
                    data,
                });
                // SAFETY: as above.
                unsafe { *effect = DROPEFFECT_COPY };
            }
            None => native.emit(Event::DragLeave { window }),
        }
        Ok(())
    }
}

impl Win32 {
    /// Accepts or refuses the drag over `id`, after each [`Event::Drag`].
    /// Only text and files can be accepted; the action is always copy.
    pub fn accept_drag(&mut self, id: WindowId, accept: bool) -> Result<(), Error> {
        self.window(id)?.drag_accept.set(Some(accept));
        Ok(())
    }

    /// Runs a copy drag of `data` from `id` until it is dropped or
    /// cancelled. This blocks in the system's drag loop, which keeps
    /// dispatching window messages; events queue until it returns.
    pub fn start_drag(&mut self, id: WindowId, data: &DragData) -> Result<(), Error> {
        let hwnd = self.window(id)?.hwnd.get();
        let (cf, bytes) = match data {
            DragData::Text(text) => {
                let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
                (CF_UNICODETEXT, unit_bytes(&units))
            }
            DragData::Files(files) => (CF_HDROP, file_list(files)),
        };
        // SAFETY: the data object takes the block when SetData succeeds,
        // otherwise it is freed here; the window is live.
        unsafe {
            let object: IDataObject = SHCreateDataObject(None, None, None::<&IDataObject>)?;
            let memory = global(bytes)?;
            let medium = STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: memory },
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            };
            if let Err(error) = object.SetData(&format(cf), &medium, true) {
                let _ = GlobalFree(Some(memory));
                return Err(error.into());
            }
            SHDoDragDrop(Some(hwnd), &object, None::<&IDropSource>, DROPEFFECT_COPY)?;
        }
        Ok(())
    }
}
