#![allow(unsafe_code)]
use crate::{Event, ImeEvent, PixelSize, Preferences, WindowId, WindowInfo, ime::Ime};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use windows::Win32::{
    Foundation::HWND,
    UI::{Input::Ime::*, WindowsAndMessaging::*},
};

pub(crate) type Queue = Rc<RefCell<VecDeque<Event>>>;

// Rc keeps this address stable from WM_NCCREATE through WM_NCDESTROY. All access
// and destruction occur on the owning UI thread. Callbacks never borrow pixels.
pub(crate) struct Native {
    pub hwnd: Cell<HWND>,
    pub id: WindowId,
    pub info: Cell<WindowInfo>,
    pub events: Queue,
    /// Last reported preferences, shared so a broadcast change emits once.
    pub preferences: Rc<Cell<Preferences>>,
    pub registered: Cell<bool>,
    pub dirty: Cell<bool>,
    pub redraw_queued: Cell<bool>,
    pub tracking: Cell<bool>,
    /// Shape shown over the client area, applied by WM_SETCURSOR.
    pub cursor: Cell<aegle_types::Cursor>,
    /// Held mouse buttons: bit 0 primary, then right, middle, back, forward.
    pub held: Cell<u8>,
    pub high_surrogate: Cell<Option<u16>>,
    pub ime: Ime,
    pub pixels: RefCell<Vec<u8>>,
    pub budget: usize,
}
/// The shared system cursor closest to `cursor`; shared cursors are never destroyed.
fn system_cursor(cursor: aegle_types::Cursor) -> HCURSOR {
    use aegle_types::Cursor;
    let name = match cursor {
        Cursor::Default => IDC_ARROW,
        Cursor::Text => IDC_IBEAM,
        // Windows has no grab cursors: the hand and the four arrows are closest.
        Cursor::Pointer | Cursor::Grab => IDC_HAND,
        Cursor::Crosshair => IDC_CROSS,
        Cursor::Move | Cursor::Grabbing => IDC_SIZEALL,
        Cursor::NotAllowed => IDC_NO,
        Cursor::ResizeHorizontal => IDC_SIZEWE,
        Cursor::ResizeVertical => IDC_SIZENS,
    };
    // SAFETY: loading a predefined cursor with no module handle has no preconditions.
    unsafe { LoadCursorW(None, name) }.unwrap_or_default()
}

impl Native {
    /// Shows this window's cursor on the calling thread's pointer.
    pub fn show_cursor(&self) {
        // SAFETY: SetCursor only changes the calling thread's current cursor.
        unsafe { SetCursor(Some(system_cursor(self.cursor.get()))) };
    }
    pub fn emit(&self, event: Event) {
        if self.registered.get() {
            self.events.borrow_mut().push_back(event);
        }
    }
    pub fn queue_redraw(&self) {
        if self.registered.get()
            && self.info.get().configured
            && self.dirty.get()
            && !self.redraw_queued.replace(true)
        {
            self.emit(Event::Redraw { window: self.id });
        }
    }
    pub fn configure(&self, physical: PixelSize, scale: f32, active: bool, drawable: bool) {
        self.info.set(WindowInfo {
            physical,
            scale,
            active,
            configured: drawable,
            size: PixelSize {
                width: (physical.width as f32 / scale).ceil() as u32,
                height: (physical.height as f32 / scale).ceil() as u32,
            },
        });
        self.dirty.set(true);
        self.emit(Event::Configure {
            window: self.id,
            info: self.info.get(),
        });
    }
    pub fn cancel_ime(&self) {
        self.ime.enabled.set(false);
        self.ime.composing.set(false);
        *self.ime.preedit.borrow_mut() = Default::default();
        // SAFETY: this context and HWND are owned on the calling UI thread.
        unsafe {
            let _ = ImmNotifyIME(self.ime.context, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
            ImmAssociateContext(self.hwnd.get(), HIMC::default());
            // Already posted composition messages belong to the old editor.
            let mut msg = MSG::default();
            while PeekMessageW(
                &mut msg,
                Some(self.hwnd.get()),
                WM_IME_STARTCOMPOSITION,
                WM_IME_COMPOSITION,
                PM_REMOVE,
            )
            .as_bool()
            {}
            while PeekMessageW(
                &mut msg,
                Some(self.hwnd.get()),
                WM_IME_CHAR,
                WM_IME_CHAR,
                PM_REMOVE,
            )
            .as_bool()
            {}
        }
        self.events.borrow_mut().retain(|event| !matches!(event, Event::Ime { window, event: ImeEvent::Update(_), .. } if *window == self.id));
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.registered.set(false);
        // SAFETY: last Rc surface lease dropped on the UI thread. The userdata
        // pointer remains valid throughout synchronous destruction callbacks.
        unsafe {
            if !self.hwnd.get().is_invalid() {
                ImmAssociateContext(self.hwnd.get(), HIMC::default());
                let _ = DestroyWindow(self.hwnd.get());
            }
        }
        // Ime's Drop destroys the disassociated context after the HWND.
    }
}
