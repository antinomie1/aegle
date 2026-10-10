#![allow(unsafe_code)]
use crate::{Error, Event, ImeEvent, PixelSize, input, native::Native, tsf::Session};
use std::panic::{AssertUnwindSafe, catch_unwind};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::ValidateRect,
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

// SAFETY: Windows invokes this only for our registered class. lpCreateParams is
// Rc::as_ptr(Native), kept alive through DestroyWindow by the last surface lease.
pub(crate) unsafe extern "system" fn procedure(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if msg == WM_NCCREATE {
            // SAFETY: WM_NCCREATE provides valid CREATESTRUCTW for this callback.
            let create = unsafe { &*(l.0 as *const CREATESTRUCTW) };
            let pointer = create.lpCreateParams as *const Native;
            // SAFETY: create_window supplied this exact stable Rc allocation.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as isize);
                (*pointer).hwnd.set(hwnd);
            }
        }
        // SAFETY: this slot contains only our pointer, cleared in `Native::drop` or at
        // WM_NCDESTROY when the system destroys the window first.
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Native;
        if pointer.is_null() {
            // SAFETY: unhandled lifecycle message forwarded unchanged.
            return unsafe { DefWindowProcW(hwnd, msg, w, l) };
        }
        // SAFETY: stable live Rc allocation described above; no mutable reference.
        let native = unsafe { &*pointer };
        match handle(native, msg, w, l) {
            Ok(Some(result)) => result,
            Ok(None) => unsafe { DefWindowProcW(hwnd, msg, w, l) },
            Err(error) => {
                native.emit(Event::Error(error));
                LRESULT(0)
            }
        }
    }));
    // No Rust unwind crosses Windows. Panics indicate an internal invariant
    // violation; continuing could leave window/input ownership inconsistent.
    match result {
        Ok(value) => value,
        Err(_) => std::process::abort(),
    }
}

fn handle(native: &Native, msg: u32, w: WPARAM, l: LPARAM) -> Result<Option<LRESULT>, Error> {
    let hwnd = native.hwnd.get();
    match msg {
        WM_CLOSE => native.emit(Event::Close { window: native.id }),
        // Color scheme, high contrast and animation changes are broadcast here.
        WM_SETTINGCHANGE => {
            let current = crate::preferences::read();
            if native.preferences.replace(current) != current {
                native.emit(Event::Preferences(current));
            }
        }
        WM_ERASEBKGND => return Ok(Some(LRESULT(1))),
        // Only the client area uses the host's cursor; borders and the title bar
        // keep the system's resize and arrow shapes.
        WM_SETCURSOR if (l.0 as u32 & 0xffff) == HTCLIENT => {
            native.show_cursor();
            return Ok(Some(LRESULT(1)));
        }
        WM_PAINT => {
            // SAFETY: acknowledging our window's invalid region; drawing happens
            // only after the host handles the queued retained-state redraw.
            unsafe {
                let _ = ValidateRect(Some(hwnd), None);
            }
            native.dirty.set(true);
            native.queue_redraw();
        }
        WM_SIZE => {
            let mut rect = RECT::default();
            // SAFETY: valid HWND, writable initialized output struct.
            unsafe {
                GetClientRect(hwnd, &mut rect)?;
            }
            let info = native.info.get();
            let physical = PixelSize {
                width: rect.right.max(0) as u32,
                height: rect.bottom.max(0) as u32,
            };
            native.configure(
                physical,
                info.scale,
                info.active,
                w.0 != SIZE_MINIMIZED as usize && physical.width > 0 && physical.height > 0,
            );
        }
        WM_DPICHANGED => {
            let mut info = native.info.get();
            info.scale = (w.0 as u16) as f32 / 96.0;
            native.info.set(info);
            // SAFETY: DPI change LPARAM is a valid suggested physical rectangle.
            let rect = unsafe { &*(l.0 as *const RECT) };
            unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )?;
            }
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            let focused = msg == WM_SETFOCUS;
            native.high_surrogate.set(None);
            let info = native.info.get();
            // SAFETY: returns the actual per-monitor DPI associated with this HWND.
            let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
            native.configure(info.physical, scale, focused, info.configured);
            native.emit(Event::KeyboardFocus {
                window: native.id,
                focused,
            });
            let enabled = native.tsf.borrow().as_ref().is_some_and(Session::enabled);
            if focused && enabled {
                native.emit(Event::Ime {
                    window: native.id,
                    event: ImeEvent::Entered,
                });
            } else if !focused {
                // The session ends without reporting its composition; already
                // queued earlier results keep their dispatch order for the host,
                // which re-enables the session when the window regains focus.
                if let Some(session) = native.tsf.borrow().as_ref() {
                    session.disable();
                }
                native.emit(Event::Ime {
                    window: native.id,
                    event: ImeEvent::Left,
                });
            }
        }
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
            input::key(native, msg, w, l);
            if msg == WM_SYSKEYDOWN || msg == WM_SYSKEYUP {
                return Ok(None);
            }
        }
        WM_CHAR => input::text(native, w.0)?,
        WM_UNICHAR => {
            if w.0 == 0xffff {
                return Ok(Some(LRESULT(1)));
            }
            let ch = char::from_u32(w.0 as u32).ok_or(Error::InvalidUtf16)?;
            if !ch.is_control() {
                native.emit(Event::Text {
                    window: native.id,
                    text: ch.to_string(),
                });
            }
        }
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_LBUTTONUP | WM_RBUTTONDOWN
        | WM_RBUTTONDBLCLK | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONDBLCLK | WM_MBUTTONUP
        | WM_MOUSELEAVE | WM_CAPTURECHANGED | WM_CANCELMODE | WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            input::pointer(native, msg, w, l)?
        }
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK | WM_XBUTTONUP => {
            input::pointer(native, msg, w, l)?;
            // Handled side buttons return TRUE, so no WM_APPCOMMAND follows.
            return Ok(Some(LRESULT(1)));
        }
        WM_NCDESTROY => {
            // SAFETY: synchronously final callback while Native still exists.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            native.hwnd.set(HWND::default());
            return Ok(None);
        }
        _ => return Ok(None),
    }
    Ok(Some(LRESULT(0)))
}
