#![allow(unsafe_code)]
use crate::{Event, ImeEvent, PixelSize, Preferences, WindowId, WindowInfo, tsf::Session};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

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
    /// The window's TSF session, from creation until the window is removed;
    /// `None` when TSF is unavailable.
    pub tsf: RefCell<Option<Session>>,
    pub pixels: RefCell<Vec<u8>>,
    /// Size of the complete frame `pixels` holds, if any.
    pub drawn: Cell<Option<crate::PixelSize>>,
    pub budget: usize,
    /// The host's answer for the current drag, `None` before one.
    pub drag_accept: Cell<Option<bool>>,
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
    /// Whether an input method is composing; its keystrokes are not host keys.
    pub fn composing(&self) -> bool {
        self.tsf.borrow().as_ref().is_some_and(Session::composing)
    }
    /// Ends the editor session; queued updates belong to the old editor.
    pub fn cancel_ime(&self) {
        if let Some(session) = self.tsf.borrow().as_ref() {
            session.disable();
        }
        self.events.borrow_mut().retain(|event| !matches!(event, Event::Ime { window, event: ImeEvent::Update(_), .. } if *window == self.id));
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.registered.set(false);
        // SAFETY: last Rc surface lease dropped on the UI thread. The userdata
        // slot is cleared first, so destruction callbacks never form a shared
        // reference to this value while `&mut self` is live; they reach
        // DefWindowProcW.
        unsafe {
            if !self.hwnd.get().is_invalid() {
                SetWindowLongPtrW(self.hwnd.get(), GWLP_USERDATA, 0);
                let _ = DestroyWindow(self.hwnd.get());
            }
        }
    }
}
