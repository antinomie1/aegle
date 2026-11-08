//! macOS desktop services against the real system, on the main thread that
//! AppKit requires, with the macOS platform's loop delivering their events.
//! It opens panels and a menu bar item, types Return, Escape and a global
//! shortcut as real keystrokes and right-clicks the item, so it is opt-in,
//! on a session left alone while it runs:
//! `cargo test -p aegle-desktop --test macos -- --ignored`.
#![allow(unsafe_code)] // Typing into the panels and pressing the status item's button.
use std::error::Error;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn Error>> {
    if !std::env::args().any(|arg| arg == "--ignored" || arg == "--include-ignored") {
        println!("macos: skipped; pass --ignored on a macOS GUI session");
        return Ok(());
    }
    macos::services()?;
    println!("macos: file panels, status item and hot keys ... ok");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod macos {
    use aegle_desktop::{Desktop, Event, FileDialog, Icon, MenuItem, Notification, Shortcut, Tray};
    use aegle_platform::Platform;
    use aegle_platform_macos::MacOs;
    use objc2_app_kit::{
        NSApplication, NSOpenPanel, NSSavePanel, NSStatusBarButton, NSView, NSWindow,
    };
    use objc2_foundation::MainThreadMarker;
    use std::{
        error::Error,
        sync::mpsc::{Receiver, channel},
        time::{Duration, Instant},
    };

    /// Runs the AppKit loop until an event arrives or `within` passes.
    fn next(backend: &mut MacOs, events: &Receiver<Event>, within: Duration) -> Option<Event> {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if let Ok(event) = events.try_recv() {
                return Some(event);
            }
            backend.dispatch(Some(Duration::from_millis(20))).ok()?;
            while backend.next_event().is_some() {}
        }
        events.try_recv().ok()
    }

    /// Runs the loop until a window of type `T` is on screen.
    fn panel<T: objc2::DowncastTarget + 'static>(
        backend: &mut MacOs,
    ) -> Option<objc2::rc::Retained<T>> {
        let app = NSApplication::sharedApplication(MainThreadMarker::new()?);
        for _ in 0..100 {
            let found = app.windows().iter().find_map(|window| {
                window
                    .isVisible()
                    .then(|| window.downcast::<T>().ok())
                    .flatten()
            });
            if found.is_some() {
                return found;
            }
            backend.dispatch(Some(Duration::from_millis(20))).ok()?;
        }
        None
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const std::ffi::c_void,
            key: u16,
            down: bool,
        ) -> *mut std::ffi::c_void;
        fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
        fn CGEventSourceCreate(state: i32) -> *mut std::ffi::c_void;
        fn CGEventCreateMouseEvent(
            source: *mut std::ffi::c_void,
            kind: u32,
            at: objc2_foundation::NSPoint,
            button: u32,
        ) -> *mut std::ffi::c_void;
        fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
        fn CFRelease(object: *mut std::ffi::c_void);
    }

    /// Types the key `code` (`kVK_…`) into `window` through the window server.
    /// Panels run out of process: they accept no `ok:` or `cancel:` and ignore
    /// events posted inside the application, so the key goes in as a real
    /// keystroke, once this application is active, `window` is key and its
    /// service has had time to fill it in.
    fn key(backend: &mut MacOs, window: &NSWindow, code: u16) {
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        for _ in 0..250 {
            if app.isActive() && window.isKeyWindow() {
                break;
            }
            let _ = backend.dispatch(Some(Duration::from_millis(20)));
        }
        assert!(
            app.isActive() && window.isKeyWindow(),
            "the panel has the keyboard"
        );
        // The panel's out-of-process service fills it in after it shows and
        // drops keys until then; nothing reports when it is ready.
        let ready = Instant::now() + Duration::from_secs(2);
        while Instant::now() < ready {
            let _ = backend.dispatch(Some(Duration::from_millis(20)));
        }
        post_key(code, &[]);
    }

    /// Presses the modifier keys `held` (`kVK_…` codes with their
    /// `CGEventFlags` bits) in order, then presses and releases `code`, then
    /// releases the modifiers, through the window server's HID tap, as typed.
    /// Posts a mouse event of `kind` at `at`, in the global top-left display
    /// space, through the window server.
    fn post_mouse(kind: u32, at: (f64, f64)) {
        // SAFETY: the source follows the HID system's state (1); the source
        // and event are released after posting to the HID tap (0).
        unsafe {
            let source = CGEventSourceCreate(1);
            let event = CGEventCreateMouseEvent(
                source,
                kind,
                objc2_foundation::NSPoint::new(at.0, at.1),
                1,
            );
            assert!(!event.is_null());
            CGEventPost(0, event);
            CFRelease(event);
            CFRelease(source);
        }
    }

    fn post_key(code: u16, held: &[(u16, u64)]) {
        let post = |code: u16, down: bool, flags: u64| {
            // SAFETY: a null source is the default one; the event is released
            // after posting to the HID tap (0).
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), code, down);
                assert!(!event.is_null());
                CGEventSetFlags(event, flags);
                CGEventPost(0, event);
                CFRelease(event);
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut flags = 0;
        for &(modifier, bit) in held {
            flags |= bit;
            post(modifier, true, flags);
        }
        post(code, true, flags);
        post(code, false, flags);
        for &(modifier, bit) in held.iter().rev() {
            flags &= !bit;
            post(modifier, false, flags);
        }
    }

    /// The status bar button in `view` or below it.
    fn find_button(
        view: objc2::rc::Retained<NSView>,
    ) -> Option<objc2::rc::Retained<NSStatusBarButton>> {
        let subviews = view.subviews();
        match view.downcast::<NSStatusBarButton>() {
            Ok(button) => Some(button),
            Err(_) => subviews.iter().find_map(find_button),
        }
    }

    pub fn services() -> Result<(), Box<dyn Error>> {
        let mut backend = MacOs::connect()?;
        let (tx, events) = channel();
        let desktop = Desktop::new("org.aegle.DesktopTest", move |event| {
            let _ = tx.send(event);
        })?;
        // AppKit belongs to the main thread; another thread is refused.
        let refused = std::thread::spawn(|| Desktop::new("org.aegle.Other", |_| {}).err())
            .join()
            .expect("the thread finishes");
        assert_eq!(
            refused.map(|error| error.kind()),
            Some(std::io::ErrorKind::Unsupported)
        );
        let wait = Duration::from_secs(5);

        // A save panel accepted as it stands returns its suggested path.
        let folder = std::env::temp_dir().join(format!("aegle-desktop-{}", std::process::id()));
        std::fs::create_dir_all(&folder)?;
        let request = desktop.save_file(&FileDialog {
            title: "Save the test file",
            name: "notes.txt",
            folder: Some(&folder),
            filters: &[("Text", &["*.txt"])],
            ..Default::default()
        })?;
        let save = panel::<NSSavePanel>(&mut backend).ok_or("no save panel appeared")?;
        // The panel shows what the dialog asked for.
        assert_eq!(
            Some(save.message().to_string()).as_deref(),
            Some("Save the test file")
        );
        assert_eq!(save.nameFieldStringValue().to_string(), "notes.txt");
        #[allow(deprecated)]
        let types = save.allowedFileTypes();
        assert_eq!(
            types.map(|types| types.iter().map(|t| t.to_string()).collect::<Vec<_>>()),
            Some(vec!["txt".to_owned()])
        );
        key(&mut backend, &save, 0x24); // Return: Save
        let answer = next(&mut backend, &events, wait);
        let Some(Event::Files {
            request: answered,
            paths: Some(paths),
        }) = answer
        else {
            return Err(format!("the save panel reported no path: {answer:?}").into());
        };
        assert_eq!(answered, request);
        assert_eq!(
            paths
                .iter()
                .map(|path| path.file_name())
                .collect::<Vec<_>>(),
            [Some("notes.txt".as_ref())]
        );
        assert_eq!(
            paths[0].parent().map(std::fs::canonicalize).transpose()?,
            Some(std::fs::canonicalize(&folder)?)
        );

        // A cancelled open panel reports no paths.
        let request = desktop.open_file(&FileDialog {
            title: "Open",
            multiple: true,
            ..Default::default()
        })?;
        let open = panel::<NSOpenPanel>(&mut backend).ok_or("no open panel appeared")?;
        assert!(open.allowsMultipleSelection() && open.canChooseFiles());
        #[allow(deprecated)]
        let types = open.allowedFileTypes();
        assert!(types.is_none(), "no filter allows every file");
        key(&mut backend, &open, 0x35); // Escape: Cancel
        assert_eq!(
            next(&mut backend, &events, wait),
            Some(Event::Files {
                request,
                paths: None
            })
        );

        // The status item reports a primary click; updating and removing it
        // keep working.
        let rgba: Vec<u8> = (0..16 * 16).flat_map(|_| [40, 120, 220, 255]).collect();
        let menu = [
            MenuItem::Item {
                id: 7,
                label: "Seven",
                enabled: true,
                checked: Some(true),
            },
            MenuItem::Separator,
            MenuItem::Submenu {
                label: "More",
                items: &[MenuItem::Item {
                    id: 8,
                    label: "Eight",
                    enabled: false,
                    checked: None,
                }],
            },
        ];
        let tray = Tray {
            icon: Icon {
                width: 16,
                height: 16,
                rgba: &rgba,
            },
            tooltip: "Aegle test",
            menu: &menu,
        };
        desktop.set_tray(Some(&tray))?;
        desktop.set_tray(Some(&Tray {
            tooltip: "Aegle test, updated",
            ..tray
        }))?;
        let mtm = MainThreadMarker::new().ok_or("not the main thread")?;
        let button = NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .find_map(|window| find_button(window.contentView()?))
            .ok_or("no status item button")?;
        assert_eq!(
            button.toolTip().map(|tip| tip.to_string()).as_deref(),
            Some("Aegle test, updated")
        );
        assert!(button.image().is_some());
        // SAFETY: a live button, clicked as the user would.
        unsafe { button.performClick(None) };
        assert_eq!(
            next(&mut backend, &events, wait),
            Some(Event::TrayActivated)
        );
        // A secondary click opens the menu, whose first item is chosen with
        // the keyboard. The menu tracks in a loop nested in event delivery,
        // so the input comes from another thread while this one runs it.
        // AppKit places the item's window in the menu bar a moment later.
        let primary = objc2_app_kit::NSScreen::screens(mtm)
            .firstObject()
            .map_or(0.0, |screen| screen.frame().size.height);
        let mut at = None;
        for _ in 0..50 {
            let frame = button.window().ok_or("the button has no window")?.frame();
            let top = primary - (frame.origin.y + frame.size.height);
            if (-1.0..1.0).contains(&top) {
                at = Some((
                    frame.origin.x + frame.size.width / 2.0,
                    top + frame.size.height / 2.0,
                ));
                break;
            }
            let _ = next(&mut backend, &events, Duration::from_millis(100));
        }
        let at = at.ok_or("the status item never reached the menu bar")?;
        let input = std::thread::spawn(move || {
            // `kCGEventRightMouseDown` and `Up`, then Down arrow and Return.
            for kind in [3, 4] {
                post_mouse(kind, at);
                std::thread::sleep(Duration::from_millis(50));
            }
            std::thread::sleep(Duration::from_millis(500));
            for code in [0x7D, 0x24] {
                post_key(code, &[]);
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        let mut chosen = None;
        let deadline = Instant::now() + wait;
        while chosen.is_none() && Instant::now() < deadline {
            chosen = next(&mut backend, &events, Duration::from_millis(50));
        }
        input.join().expect("the input thread finishes");
        assert_eq!(chosen, Some(Event::TrayMenu { item: 7 }));
        desktop.set_tray(None)?;

        // Hot keys register, a key Mac keyboards lack is reported, and
        // rebinding replaces them.
        let shortcuts = [
            Shortcut {
                id: "show",
                description: "Show the test window",
                trigger: "Ctrl+Alt+Shift+F19".parse()?,
            },
            Shortcut {
                id: "missing",
                description: "A key Macs lack",
                trigger: "Ctrl+F24".parse()?,
            },
        ];
        desktop.bind_shortcuts(&shortcuts)?;
        let Some(Event::Unavailable(reason)) = next(&mut backend, &events, wait) else {
            return Err("F24 was not reported".into());
        };
        assert!(reason.contains("missing"), "{reason}");
        // Control, Option and Shift with F19 (`kVK_F19`), as typed: a Mac
        // keyboard's function keys carry the Fn flag, which the window
        // server's hot-key match expects.
        post_key(
            0x50,
            &[
                (0x3B, 0x40000),
                (0x3A, 0x80000),
                (0x38, 0x20000),
                (0x3F, 0x800000),
            ],
        );
        assert_eq!(
            next(&mut backend, &events, wait),
            Some(Event::Shortcut { id: "show".into() })
        );
        desktop.bind_shortcuts(&shortcuts[..1])?;
        desktop.bind_shortcuts(&[])?;
        assert_eq!(
            next(&mut backend, &events, Duration::from_millis(200)),
            None
        );

        // A process outside an application bundle has no notification
        // center, and says so instead of failing.
        desktop.notify(&Notification {
            summary: "Hello",
            ..Default::default()
        })?;
        let Some(Event::Unavailable(reason)) = next(&mut backend, &events, wait) else {
            return Err("notify reported nothing".into());
        };
        assert!(reason.contains("bundle"), "{reason}");
        drop(desktop);
        std::fs::remove_dir_all(&folder)?;
        Ok(())
    }
}
