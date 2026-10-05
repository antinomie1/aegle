//! Native lifecycle contract, intentionally opt-in on a test desktop.
#![cfg(windows)]
#![allow(unsafe_code)]
use aegle_platform_win32::{
    Error, Event, ImeRequest, PixelSize, PresentError, Win32, WindowOptions,
};
use aegle_types::Rect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::Duration;
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::{
        Input::Ime::{ImmGetContext, ImmReleaseContext},
        WindowsAndMessaging::*,
    },
};

#[test]
#[ignore = "creates native windows; run on a dedicated Windows/Wine test desktop"]
fn native_lifecycle_pixels_input_and_owned_surface() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = Win32::connect()?;
    assert!(Win32::connect().is_err());
    let first = backend.create_window(WindowOptions {
        size: PixelSize {
            width: 96,
            height: 64,
        },
        buffer_budget: 4 * 1024 * 1024,
        ..Default::default()
    })?;
    let second = backend.create_window(WindowOptions {
        title: "Aegle second",
        ..Default::default()
    })?;
    let lease = backend.window_surface(first)?;
    let RawWindowHandle::Win32(handle) = lease.window_handle().unwrap().as_raw() else {
        unreachable!()
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    // SAFETY: hwnd is retained by the owned surface lease throughout these calls.
    unsafe {
        assert!(IsWindow(Some(hwnd)).as_bool());
        assert!(!IsWindowVisible(hwnd).as_bool());
    }
    while backend.next_event().is_some() {}
    let mut draws = 0;
    assert!(backend.present(first, |pixels, _| {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[20, 100, 220, 255]);
        }
        draws += 1;
        Ok::<_, std::convert::Infallible>(())
    })?);
    assert_eq!(draws, 1);
    assert!(backend.buffer_bytes(first)? <= 4 * 1024 * 1024);
    unsafe {
        assert!(IsWindowVisible(hwnd).as_bool());
    }
    assert!(matches!(
        backend.present(first, |pixels, _| {
            pixels.fill(0);
            Ok::<_, std::convert::Infallible>(())
        }),
        Err(PresentError::Platform(Error::UnsupportedTransparency))
    ));
    backend.configure_ime(
        first,
        Some(ImeRequest {
            cursor_rect: Rect::new(4.0, 8.0, 1.0, 20.0),
        }),
    )?;
    // SAFETY: native HIMC is borrowed only between its acquire/release calls.
    unsafe {
        let context = ImmGetContext(hwnd);
        assert!(!context.is_invalid());
        assert!(ImmReleaseContext(hwnd, context).as_bool());
    }
    backend.configure_ime(first, None)?;
    while backend.next_event().is_some() {}
    // SAFETY: synthesize documented WM_CHAR UTF-16 messages to this test window.
    unsafe {
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0xd83d)), Some(LPARAM(0)));
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0xde42)), Some(LPARAM(0)));
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0x754c)), Some(LPARAM(0)));
    }
    let mut input = String::new();
    while let Some(event) = backend.next_event() {
        match event {
            Event::Text { text, .. } => input.push_str(&text),
            Event::Error(e) => return Err(e.into()),
            _ => {}
        }
    }
    assert_eq!(input, "🙂界");
    // A ready animation frame must not starve an already posted native input.
    backend.request_redraw(first)?;
    unsafe {
        PostMessageW(Some(hwnd), WM_CHAR, WPARAM(0x41), LPARAM(0)).map_err(Error::from)?;
    }
    let mut delivered = false;
    for _ in 0..32 {
        backend.dispatch(Some(Duration::ZERO))?;
        while let Some(event) = backend.next_event() {
            if matches!(event, Event::Text { text, .. } if text == "A") {
                delivered = true;
            }
        }
        if delivered {
            break;
        }
    }
    assert!(delivered);
    // SAFETY: resize the owned native toplevel; WM_SIZE must publish actual client geometry.
    unsafe {
        SetWindowPos(hwnd, None, 0, 0, 280, 180, SWP_NOMOVE | SWP_NOZORDER).map_err(Error::from)?;
    }
    assert!(backend.window_info(first)?.buffer_size()?.width > 96);
    backend.remove_window(first)?;
    // Native ownership, not route membership, governs GPU surface validity.
    unsafe {
        assert!(IsWindow(Some(hwnd)).as_bool());
        assert!(!IsWindowVisible(hwnd).as_bool());
    }
    assert!(backend.window_info(first).is_err());
    drop(lease);
    unsafe {
        assert!(!IsWindow(Some(hwnd)).as_bool());
    }
    backend.set_clipboard(second, "剪贴板 😀")?;
    assert_eq!(
        backend.clipboard_text(second)?.as_deref(),
        Some("剪贴板 😀")
    );
    backend.remove_window(second)?;
    let wake = backend.wake_handle()?;
    std::thread::spawn(move || wake.wake()).join().unwrap();
    backend.dispatch(Some(Duration::from_secs(1)))?;
    let mut event = backend.next_event();
    if event.is_none() {
        // Wine may first dispatch messages left by the clipboard owner; the wake stays signaled.
        backend.dispatch(Some(Duration::from_secs(1)))?;
        event = backend.next_event();
    }
    assert!(matches!(event, Some(Event::Wake)));
    println!("Win32 native lifecycle, pixels, Unicode, IMM context and owned surface passed");
    Ok(())
}
