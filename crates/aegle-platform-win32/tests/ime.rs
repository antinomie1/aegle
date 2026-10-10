//! Real input-method contract: Microsoft Pinyin through the window's TSF
//! text store. It synthesizes keystrokes into the foreground window, so it runs only
//! on a dedicated Windows test desktop with the Chinese (Simplified) Microsoft
//! Pinyin input method installed:
//! `cargo test -p aegle-platform-win32 --test ime -- --ignored`.
#![cfg(windows)]
#![allow(unsafe_code)]
use aegle_platform_win32::{
    Error, Event, ImeEvent, ImeRequest, PixelSize, Win32, WindowId, WindowOptions,
};
use aegle_types::Rect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::HWND,
    System::{
        Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        Threading::{AttachThreadInput, GetCurrentThreadId},
        Variant::VARIANT,
    },
    UI::{
        Input::KeyboardAndMouse::*,
        TextServices::{CLSID_TF_ThreadMgr, GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, ITfThreadMgr},
        WindowsAndMessaging::*,
    },
};

/// Microsoft Pinyin is the default input method of the Simplified Chinese layout.
const SIMPLIFIED_CHINESE: windows::core::PCWSTR = windows::core::w!("00000804");

fn foreground(hwnd: HWND) {
    // SAFETY: the window is live and owned by this thread; the foreground
    // thread's input is attached only for the duration of the activation.
    unsafe {
        let other = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let current = GetCurrentThreadId();
        let attached = other != current && AttachThreadInput(other, current, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if attached {
            let _ = AttachThreadInput(other, current, false);
        }
    }
}

/// Sends key taps, refusing to type unless the test window owns the foreground.
fn tap(hwnd: HWND, keys: &[VIRTUAL_KEY]) {
    for &key in keys {
        // SAFETY: reads global foreground state only.
        assert_eq!(
            unsafe { GetForegroundWindow() },
            hwnd,
            "the test window lost the foreground; refusing to type elsewhere"
        );
        let input = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        let inputs = [input(KEYBD_EVENT_FLAGS(0)), input(KEYEVENTF_KEYUP)];
        // SAFETY: the array holds initialized keyboard INPUT records.
        assert_eq!(unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) }, 2);
    }
}

fn letters(text: &str) -> Vec<VIRTUAL_KEY> {
    text.bytes()
        .map(|b| VIRTUAL_KEY(b.to_ascii_uppercase() as u16))
        .collect()
}

/// IME updates, focus changes and plain text received until `done` holds or a timeout.
#[derive(Default)]
struct Seen {
    preedits: Vec<String>,
    commits: String,
    text: String,
    keys: usize,
    left: bool,
    focused: bool,
}

impl Seen {
    fn record(&mut self, window: WindowId, event: Event) {
        match event {
            Event::Ime {
                window: w,
                event: ImeEvent::Update(update),
            } if w == window => {
                self.commits.extend(update.commit);
                self.preedits.push(update.preedit.text);
            }
            Event::Ime {
                window: w,
                event: ImeEvent::Left,
            } if w == window => self.left = true,
            Event::KeyboardFocus {
                window: w,
                focused: true,
            } if w == window => self.focused = true,
            Event::Text { text, .. } => self.text.push_str(&text),
            Event::Key { pressed: true, .. } => self.keys += 1,
            Event::Error(e) => panic!("{e}"),
            _ => {}
        }
    }
}

fn pump(backend: &mut Win32, window: WindowId, done: impl Fn(&Seen) -> bool) -> Seen {
    let mut seen = Seen::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(&seen) {
        assert!(Instant::now() < deadline, "input method did not respond");
        backend.dispatch(Some(Duration::from_millis(20))).unwrap();
        while let Some(event) = backend.next_event() {
            seen.record(window, event);
        }
    }
    // Let trailing edits for this keystroke arrive.
    let settle = Instant::now() + Duration::from_millis(150);
    while Instant::now() < settle {
        backend.dispatch(Some(Duration::from_millis(20))).unwrap();
        while let Some(event) = backend.next_event() {
            seen.record(window, event);
        }
    }
    seen
}

#[test]
#[ignore = "types into the foreground window with Microsoft Pinyin; run on a dedicated Windows test desktop"]
fn microsoft_pinyin_preedit_commit_cancel_and_focus_loss() -> Result<(), Box<dyn std::error::Error>>
{
    let mut backend = Win32::connect()?;
    let window = backend.create_window(WindowOptions {
        title: "Aegle IME",
        size: PixelSize {
            width: 320,
            height: 120,
        },
        ..Default::default()
    })?;
    let lease = backend.window_surface(window)?;
    let RawWindowHandle::Win32(handle) = lease.window_handle().unwrap().as_raw() else {
        unreachable!()
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    // A second window takes the focus later; showing it activates it, so it
    // is shown first.
    let other = backend.create_window(WindowOptions {
        title: "Aegle IME other",
        size: PixelSize {
            width: 160,
            height: 80,
        },
        ..Default::default()
    })?;
    let other_lease = backend.window_surface(other)?;
    let RawWindowHandle::Win32(handle) = other_lease.window_handle().unwrap().as_raw() else {
        unreachable!()
    };
    let other_hwnd = HWND(handle.hwnd.get() as *mut _);
    for id in [other, window] {
        assert!(backend.present(id, None, |pixels, _, _| {
            pixels.fill(255);
            Ok::<_, std::convert::Infallible>(())
        })?);
    }
    foreground(hwnd);
    let request = ImeRequest {
        cursor_rect: Rect::new(12.0, 16.0, 1.0, 20.0),
        ..Default::default()
    };
    backend.configure_ime(window, Some(request.clone()))?;
    // SAFETY: activates a system layout for this thread and opens its input
    // method through the thread's TSF keyboard compartment.
    unsafe {
        let layout = LoadKeyboardLayoutW(SIMPLIFIED_CHINESE, KLF_ACTIVATE).map_err(Error::from)?;
        ActivateKeyboardLayout(layout, KLF_SETFORPROCESS).map_err(Error::from)?;
        let manager: ITfThreadMgr =
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)
                .map_err(Error::from)?;
        let client = manager.Activate().map_err(Error::from)?;
        let open = manager
            .GetGlobalCompartment()
            .and_then(|c| c.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE))
            .map_err(Error::from)?;
        open.SetValue(client, &VARIANT::from(1i32))
            .map_err(Error::from)?;
        manager.Deactivate().map_err(Error::from)?;
    }
    pump(&mut backend, window, |_| true);

    // Composition: the IME owns the keystrokes and the host sees only preedit.
    tap(hwnd, &letters("nihao"));
    let seen = pump(&mut backend, window, |s| {
        s.preedits
            .last()
            .is_some_and(|p| p.replace(['\'', ' '], "") == "nihao")
    });
    assert!(
        seen.commits.is_empty() && seen.text.is_empty(),
        "{:?}",
        seen.text
    );
    assert_eq!(seen.keys, 0, "composition keystrokes reached the host");
    // The first candidate is committed once, as a TSF edit and never also as
    // WM_CHAR text.
    tap(hwnd, &[VK_SPACE]);
    let seen = pump(&mut backend, window, |s| !s.commits.is_empty());
    assert_eq!(seen.commits, "你好");
    assert!(seen.text.is_empty(), "WM_CHAR duplicated the commit");
    assert_eq!(seen.preedits.last().map(String::as_str), Some(""));

    // Escape cancels a composition without committing.
    tap(hwnd, &letters("zhong"));
    pump(&mut backend, window, |s| !s.preedits.is_empty());
    tap(hwnd, &[VK_ESCAPE]);
    let seen = pump(&mut backend, window, |s| {
        s.preedits.last().is_some_and(String::is_empty)
    });
    assert!(seen.commits.is_empty() && seen.text.is_empty());

    // Disabling the session (editor change or focus loss) drops the preedit
    // and any queued result; nothing from the old composition arrives later.
    tap(hwnd, &letters("shi"));
    pump(&mut backend, window, |s| !s.preedits.is_empty());
    backend.configure_ime(window, None)?;
    let seen = pump(&mut backend, window, |_| true);
    assert!(
        seen.commits.is_empty() && seen.preedits.is_empty(),
        "{:?}",
        seen.preedits
    );
    // Re-enabling starts a fresh composition in the same window; the text the
    // input method kept when its composition was terminated is not committed.
    backend.configure_ime(window, Some(request.clone()))?;
    tap(hwnd, &letters("zhongwen"));
    let seen = pump(&mut backend, window, |s| {
        s.preedits
            .last()
            .is_some_and(|p| p.replace(['\'', ' '], "") == "zhongwen")
    });
    assert!(seen.commits.is_empty(), "{:?}", seen.commits);
    tap(hwnd, &[VK_SPACE]);
    let seen = pump(&mut backend, window, |s| !s.commits.is_empty());
    assert_eq!(seen.commits, "中文");

    // Losing the window focus ends the session without committing.
    tap(hwnd, &letters("ri"));
    pump(&mut backend, window, |s| !s.preedits.is_empty());
    foreground(other_hwnd);
    let seen = pump(&mut backend, window, |s| s.left);
    assert!(seen.commits.is_empty(), "{:?}", seen.commits);
    // A host re-enables its focused editor when the window regains focus.
    foreground(hwnd);
    pump(&mut backend, window, |s| s.focused);
    backend.configure_ime(window, Some(request))?;
    tap(hwnd, &letters("nihao"));
    let seen = pump(&mut backend, window, |s| {
        s.preedits
            .last()
            .is_some_and(|p| p.replace(['\'', ' '], "") == "nihao")
    });
    assert!(seen.commits.is_empty(), "{:?}", seen.commits);
    tap(hwnd, &[VK_SPACE]);
    let seen = pump(&mut backend, window, |s| !s.commits.is_empty());
    assert_eq!(seen.commits, "你好");

    backend.remove_window(other)?;
    backend.remove_window(window)?;
    drop((lease, other_lease));
    println!("Microsoft Pinyin preedit, commit, cancel, session reset and focus loss passed");
    Ok(())
}
