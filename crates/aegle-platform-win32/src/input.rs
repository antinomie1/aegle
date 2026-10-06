#![allow(unsafe_code)]
use crate::{Error, Event, Modifiers, PointerKind, native::Native};
use aegle_types::Point;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::{
    Foundation::{LPARAM, POINT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub(crate) fn modifiers() -> Modifiers {
    // SAFETY: GetKeyState reads this UI thread's message-queue keyboard state.
    unsafe {
        Modifiers {
            shift: GetKeyState(VK_SHIFT.0 as i32) < 0,
            control: GetKeyState(VK_CONTROL.0 as i32) < 0,
            alt: GetKeyState(VK_MENU.0 as i32) < 0,
            meta: GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0,
        }
    }
}

/// The time of the message being processed, in milliseconds since boot.
fn message_time() -> u32 {
    // SAFETY: GetMessageTime reads this UI thread's current message state.
    unsafe { GetMessageTime() as u32 }
}

pub(crate) fn key(native: &Native, msg: u32, w: WPARAM, l: LPARAM) {
    // IMM owns VK_PROCESSKEY and composition keystrokes. Forwarding these to
    // the Editor would move/delete text while the input method edits preedit.
    if w.0 == VK_PROCESSKEY.0 as usize || native.ime.composing.get() {
        return;
    }
    native.emit(Event::Key {
        window: native.id,
        key: w.0 as u32,
        pressed: matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN),
        repeat: l.0 & (1 << 30) != 0,
        modifiers: modifiers(),
        time: message_time(),
    });
}

pub(crate) fn text(native: &Native, value: usize) -> Result<(), Error> {
    let unit = u16::try_from(value).map_err(|_| Error::InvalidUtf16)?;
    // Enter, tab and deletion are key commands, not duplicate text insertions.
    if unit < 0x20 || unit == 0x7f {
        return Ok(());
    }
    let high = native.high_surrogate.take();
    let text = if (0xd800..=0xdbff).contains(&unit) {
        if high.is_some() {
            return Err(Error::InvalidUtf16);
        }
        native.high_surrogate.set(Some(unit));
        return Ok(());
    } else if let Some(high) = high {
        String::from_utf16(&[high, unit]).map_err(|_| Error::InvalidUtf16)?
    } else {
        String::from_utf16(&[unit]).map_err(|_| Error::InvalidUtf16)?
    };
    native.emit(Event::Text {
        window: native.id,
        text,
    });
    Ok(())
}

pub(crate) fn pointer(native: &Native, msg: u32, w: WPARAM, l: LPARAM) -> Result<(), Error> {
    let mut point = POINT {
        x: l.0 as i16 as i32,
        y: (l.0 >> 16) as i16 as i32,
    };
    let kind = match msg {
        WM_MOUSEMOVE => {
            if !native.tracking.replace(true) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: native.hwnd.get(),
                    dwHoverTime: 0,
                };
                // SAFETY: stack structure has correct size and a live owned HWND.
                unsafe {
                    TrackMouseEvent(&mut track)?;
                }
            }
            PointerKind::Move
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            native.pressed.set(true);
            // SAFETY: capture and focus are confined to the current live HWND.
            unsafe {
                SetCapture(native.hwnd.get());
                let _ = SetFocus(Some(native.hwnd.get()));
            }
            PointerKind::Down {
                clicks: if msg == WM_LBUTTONDBLCLK { 2 } else { 1 },
            }
        }
        WM_LBUTTONUP => {
            native.pressed.set(false);
            // SAFETY: release our capture; synchronous WM_CAPTURECHANGED observes
            // pressed=false and cannot synthesize a cancel before this release.
            unsafe {
                ReleaseCapture()?;
            }
            PointerKind::Up
        }
        WM_MOUSELEAVE => {
            native.tracking.set(false);
            if native.pressed.get() {
                return Ok(());
            }
            PointerKind::Leave
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            if !native.pressed.replace(false) {
                return Ok(());
            }
            PointerKind::Leave
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            // SAFETY: OS mouse-wheel LPARAM uses screen pixels, output is a local POINT.
            if !unsafe { ScreenToClient(native.hwnd.get(), &mut point) }.as_bool() {
                return Err(Error::Backend("ScreenToClient failed".into()));
            }
            let horizontal = msg == WM_MOUSEHWHEEL;
            let mut units = 0u32;
            // SAFETY: SPI wheel setting writes a DWORD to this aligned local.
            unsafe {
                SystemParametersInfoW(
                    if horizontal {
                        SPI_GETWHEELSCROLLCHARS
                    } else {
                        SPI_GETWHEELSCROLLLINES
                    },
                    0,
                    Some((&mut units as *mut u32).cast()),
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                )?;
            }
            let info = native.info.get();
            let distance = if units == u32::MAX {
                (if horizontal {
                    info.size.width
                } else {
                    info.size.height
                }) as f32
            } else {
                units as f32 * 16.0
            };
            let amount = (w.0 >> 16) as i16 as f32 / WHEEL_DELTA as f32 * distance;
            PointerKind::Scroll {
                delta: if horizontal {
                    Point::new(amount, 0.0)
                } else {
                    Point::new(0.0, -amount)
                },
            }
        }
        _ => unreachable!(),
    };
    let scale = native.info.get().scale;
    native.emit(Event::Pointer {
        window: native.id,
        position: Point::new(point.x as f32 / scale, point.y as f32 / scale),
        kind,
        modifiers: modifiers(),
        time: message_time(),
    });
    Ok(())
}
