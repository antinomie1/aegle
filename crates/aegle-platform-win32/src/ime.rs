#![allow(unsafe_code)]
use crate::{
    Error, Event, ImeEvent, ImeRequest, ImeUpdate, Preedit, ime_types::utf16_cursor, native::Native,
};
use std::cell::{Cell, RefCell};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    UI::Input::Ime::*,
};

pub(crate) struct Ime {
    pub context: HIMC,
    pub enabled: Cell<bool>,
    pub composing: Cell<bool>,
    pub preedit: RefCell<Preedit>,
}
impl Ime {
    pub fn new() -> Result<Self, Error> {
        // SAFETY: creates a context owned and destroyed on this UI thread.
        let context = unsafe { ImmCreateContext() };
        if context.is_invalid() {
            return Err(Error::Backend("ImmCreateContext failed".into()));
        }
        Ok(Self {
            context,
            enabled: Cell::new(false),
            composing: Cell::new(false),
            preedit: RefCell::default(),
        })
    }
    fn string(&self, kind: IME_COMPOSITION_STRING) -> Result<String, Error> {
        // SAFETY: valid owned context, first query has no output buffer.
        let length = unsafe { ImmGetCompositionStringW(self.context, kind, None, 0) };
        if length < 0 || length % 2 != 0 || length > 1024 * 1024 {
            return Err(Error::InvalidIme(
                "invalid or oversized composition payload",
            ));
        }
        let mut units = vec![0u16; length as usize / 2];
        // SAFETY: allocated exact byte extent, aligned UTF-16; the same UI-thread
        // context cannot be changed by another input dispatch during this call.
        let received = unsafe {
            ImmGetCompositionStringW(
                self.context,
                kind,
                Some(units.as_mut_ptr().cast()),
                length as u32,
            )
        };
        if received != length {
            return Err(Error::InvalidIme("composition changed during read"));
        }
        String::from_utf16(&units).map_err(|_| Error::InvalidUtf16)
    }
    pub fn update(&self, flags: u32) -> Result<ImeUpdate, Error> {
        let commit = if flags & GCS_RESULTSTR.0 != 0 {
            Some(self.string(GCS_RESULTSTR)?)
        } else {
            None
        };
        let mut preedit = self.preedit.borrow().clone();
        if commit.is_some() {
            preedit = Preedit::default();
        }
        if flags & GCS_COMPSTR.0 != 0 {
            preedit.text = self.string(GCS_COMPSTR)?;
        }
        if flags & (GCS_COMPSTR.0 | GCS_CURSORPOS.0) != 0 {
            // SAFETY: cursor query has no output buffer and the owned HIMC is live.
            let pos = unsafe { ImmGetCompositionStringW(self.context, GCS_CURSORPOS, None, 0) };
            preedit.cursor = if pos < 0 {
                None
            } else {
                let byte = utf16_cursor(&preedit.text, pos as usize)?;
                Some((byte, byte))
            };
        }
        if flags == 0 {
            preedit = Preedit::default();
        }
        *self.preedit.borrow_mut() = preedit.clone();
        self.composing.set(!preedit.text.is_empty());
        Ok(ImeUpdate { commit, preedit })
    }
}
impl Drop for Ime {
    fn drop(&mut self) {
        // SAFETY: Native disassociates this owned context before field destruction.
        unsafe {
            let _ = ImmDestroyContext(self.context);
        }
    }
}

pub(crate) fn configure(native: &Native, request: Option<ImeRequest>) -> Result<(), Error> {
    let Some(request) = request else {
        native.cancel_ime();
        return Ok(());
    };
    let [left, top, right, bottom] = request.validate(native.info.get().scale)?;
    if !native.ime.enabled.replace(true) {
        // SAFETY: custom per-window context and HWND share the calling UI thread.
        unsafe {
            ImmAssociateContext(native.hwnd.get(), native.ime.context);
        }
        if native.info.get().active {
            native.emit(Event::Ime {
                window: native.id,
                event: ImeEvent::Entered,
            });
        }
    }
    let rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    let candidate = CANDIDATEFORM {
        dwIndex: 0,
        dwStyle: CFS_EXCLUDE,
        ptCurrentPos: POINT { x: left, y: bottom },
        rcArea: rect,
    };
    let composition = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: POINT { x: left, y: top },
        rcArea: rect,
    };
    // SAFETY: these structs contain client-pixel coordinates checked at the boundary.
    unsafe {
        if !ImmSetCandidateWindow(native.ime.context, &candidate).as_bool()
            || !ImmSetCompositionWindow(native.ime.context, &composition).as_bool()
        {
            return Err(Error::Backend("IMM candidate positioning failed".into()));
        }
    }
    Ok(())
}

pub(crate) fn suppress_system_composition(
    hwnd: HWND,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: forwarding original callback parameters, masking only the system
    // preedit window because the shared retained Editor paints that text itself.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_IME_SETCONTEXT,
            wparam,
            windows::Win32::Foundation::LPARAM(lparam.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize)),
        )
    }
}
