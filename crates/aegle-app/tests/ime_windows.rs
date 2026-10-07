//! Microsoft Pinyin through a native App window into a retained TextField.
//! It synthesizes keystrokes into the foreground window, so it runs only on a
//! dedicated Windows test desktop with Simplified Chinese Microsoft Pinyin:
//! `AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-app --features
//! windows,software --test ime_windows -- --ignored`.
#![cfg(all(feature = "windows", feature = "software", target_os = "windows"))]
#![allow(unsafe_code)]

use aegle_app::{App, AppOptions, WindowOptions};
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{Result, TextSystem};
use aegle_widgets::Widgets;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::{
            Input::{Ime::*, KeyboardAndMouse::*},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

/// Sends key taps, refusing to type unless the test window owns the foreground.
fn tap(hwnd: HWND, text: &str) {
    for byte in text.bytes() {
        let key = match byte {
            b' ' => VK_SPACE,
            0x1b => VK_ESCAPE,
            _ => VIRTUAL_KEY(u16::from(byte.to_ascii_uppercase())),
        };
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

fn run(app: &App, until: impl Fn() -> bool) -> Result {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until() {
        assert!(Instant::now() < deadline, "input method did not respond");
        app.dispatch(Some(Duration::from_millis(20)))?;
    }
    // Trailing composition messages for the last keystroke.
    let settle = Instant::now() + Duration::from_millis(150);
    while Instant::now() < settle {
        app.dispatch(Some(Duration::from_millis(20)))?;
    }
    Ok(())
}

#[test]
#[ignore = "types into the foreground window with Microsoft Pinyin; run on a dedicated Windows test desktop"]
fn microsoft_pinyin_commits_into_the_focused_text_field() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut fonts = TextSystem::new();
    let family = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?[0]
        .0;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, [family].into_iter());
    let app = App::with_fonts(fonts, AppOptions::default())?;
    let window = app.window_with_options(
        "Aegle IME field",
        WindowOptions {
            width: 320,
            height: 160,
            ..Default::default()
        },
    )?;
    let field = window.text_field("")?;
    let button = window.button("Other")?;
    // SAFETY: looks up this process's uniquely titled window.
    let find = || unsafe { FindWindowW(None, w!("Aegle IME field")).unwrap_or_default() };
    run(&app, || {
        // SAFETY: visibility query on a possibly null handle.
        !find().is_invalid() && unsafe { IsWindowVisible(find()) }.as_bool()
    })?;
    let hwnd = find();
    // SAFETY: the window belongs to this thread; the foreground thread's input
    // is attached only for the activation. The layout is activated for this
    // process and the window's own HIMC is borrowed between acquire/release.
    unsafe {
        let other = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let current = GetCurrentThreadId();
        let attached = other != current && AttachThreadInput(other, current, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if attached {
            let _ = AttachThreadInput(other, current, false);
        }
        let layout = LoadKeyboardLayoutW(w!("00000804"), KLF_ACTIVATE).unwrap();
        ActivateKeyboardLayout(layout, KLF_SETFORPROCESS).unwrap();
    }
    field.focus()?;
    run(&app, || true)?;
    // SAFETY: as above; the App associated its context when the field focused.
    unsafe {
        let context = ImmGetContext(hwnd);
        assert!(
            !context.is_invalid(),
            "the focused field has no input context"
        );
        assert!(ImmSetOpenStatus(context, true).as_bool());
        assert!(
            ImmSetConversionStatus(context, IME_CMODE_NATIVE, IME_SMODE_PHRASEPREDICT).as_bool()
        );
        let _ = ImmReleaseContext(hwnd, context);
    }

    // Composition keystrokes are neither inserted nor committed until selection.
    tap(hwnd, "nihao");
    run(&app, || true)?;
    assert_eq!(field.text()?, "");
    tap(hwnd, " ");
    run(&app, || field.text().unwrap() == "你好")?;
    assert_eq!(field.text()?, "你好");

    // Escape cancels the composition, not the field.
    tap(hwnd, "zhong\x1b");
    run(&app, || true)?;
    assert_eq!(field.text()?, "你好");

    // Moving focus away ends the session without committing its preedit.
    tap(hwnd, "shi");
    run(&app, || true)?;
    button.focus()?;
    run(&app, || true)?;
    assert_eq!(field.text()?, "你好");

    // Returning starts a fresh session that appends at the caret.
    field.focus()?;
    run(&app, || true)?;
    tap(hwnd, "zhongwen ");
    run(&app, || field.text().unwrap() == "你好中文")?;
    window.close()?;
    Ok(())
}
