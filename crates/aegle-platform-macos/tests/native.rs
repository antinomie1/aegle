//! Native lifecycle contract, intentionally opt-in on a macOS GUI session:
//! `cargo test -p aegle-platform-macos --test native -- --ignored`. The drag
//! scenario moves the real pointer, so leave the session alone while it runs.
//!
//! AppKit runs on the main thread only, which libtest never gives a test, so
//! this target has its own `main` and runs its scenarios in order.
#![allow(unsafe_code)] // Borrowing the leased view to post events to it.
#[cfg(target_os = "macos")]
type Outcome = Result<(), Box<dyn std::error::Error>>;

#[cfg(target_os = "macos")]
fn main() -> Outcome {
    if !std::env::args().any(|arg| arg == "--ignored" || arg == "--include-ignored") {
        println!("native: skipped; pass --ignored on a macOS GUI session");
        return Ok(());
    }
    for (name, scenario) in [
        (
            "lifecycle, pixels and owned surface",
            macos::lifecycle as fn() -> Outcome,
        ),
        ("outputs and popups", macos::outputs_and_popups),
        ("drag and drop between windows", macos::drag_between_windows),
    ] {
        scenario().map_err(|error| format!("{name}: {error}"))?;
        println!("native: {name} ... ok");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod macos {
    use super::Outcome;
    use aegle_platform::{
        Error, Event, ImeRequest, PixelSize, Platform, PopupOptions, Rect, SeatId, Side,
        SurfacePlatform, WindowOptions,
    };
    use aegle_platform_macos::MacOs;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::time::Duration;

    /// Presses nothing and takes the window down again: the contract's window,
    /// presentation, wake, IME and error paths.
    pub fn lifecycle() -> Outcome {
        let mut backend = MacOs::connect()?;
        assert!(MacOs::connect().is_err(), "one loop per process");
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
        assert_ne!(first, second, "identities are never reused");
        let lease = backend.window_surface(first)?;
        let handle = lease
            .window_handle()
            .map_err(|error| format!("no window handle: {error}"))?
            .as_raw();
        assert!(matches!(handle, RawWindowHandle::AppKit(_)));
        // The rented view stays alive and unchanged while the lease exists.
        {
            let RawWindowHandle::AppKit(handle) = handle else {
                unreachable!()
            };
            let _ = handle.ns_view.as_ptr();
        }
        while backend.next_event().is_some() {}
        let info = backend.window_info(first)?;
        assert!(info.buffer.width >= 96 && info.buffer.height >= 64);
        assert!(info.scale >= 1.0);
        // Hidden until the first frame can be shown.
        let mut draws = 0;
        assert!(backend.present(first, None, |pixels, _, _| {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.copy_from_slice(&[20, 100, 220, 255]);
            }
            draws += 1;
            Ok(())
        })?);
        assert_eq!(draws, 1);
        // The window stayed opaque: the alpha byte is ignored, not rejected.
        assert!(backend.present(first, None, |pixels, _, _| {
            pixels.fill(0);
            Ok(())
        })?);
        // A second window presents independently.
        assert!(backend.present(second, None, |_, _, _| Ok(()))?);
        // A drawing error presents nothing and keeps the redraw request.
        assert!(
            backend
                .present(first, None, |_, _, _| Err("draw failed".into()))
                .is_err()
        );
        assert!(backend.present(first, None, |_, _, _| Ok(()))?);
        // Input methods are always available; the request is validated once.
        assert!(backend.ime_available());
        let invalid = ImeRequest {
            surrounding: Some("a\0b".into()),
            cursor_rect: Rect::new(4.0, 8.0, 1.0, 20.0),
            ..Default::default()
        };
        assert!(matches!(
            backend.configure_ime(first, Some(invalid)),
            Err(Error::InvalidIme(_))
        ));
        backend.configure_ime(
            first,
            Some(ImeRequest {
                surrounding: Some("hello".into()),
                cursor: 5,
                anchor: 2,
                cursor_rect: Rect::new(4.0, 8.0, 1.0, 20.0),
                multiline: false,
                input_method: true,
            }),
        )?;
        while let Some(event) = backend.next_event() {
            assert!(!matches!(event, Event::Error(_)), "{event:?}");
        }
        // An input method may commit from a run-loop callback instead of a key
        // press, as a candidate clicked with the mouse does; that ends a
        // blocking wait like an event.
        {
            use objc2_foundation::{NSRange, NSRunLoop, NSString};
            let RawWindowHandle::AppKit(handle) = handle else {
                unreachable!()
            };
            // SAFETY: the lease keeps the view alive on this, the main thread.
            let view: objc2::rc::Retained<objc2_app_kit::NSView> = objc2::Message::retain(unsafe {
                handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref()
            });
            let commit = block2::RcBlock::new(move || {
                let text = NSString::from_str("中文");
                let replacement = NSRange::new(isize::MAX as usize, 0);
                // SAFETY: the view implements NSTextInputClient.
                let () = unsafe {
                    objc2::msg_send![&*view, insertText: &*text, replacementRange: replacement]
                };
            });
            settle(&mut backend)?;
            // SAFETY: the block runs on the main thread, where the view lives.
            unsafe { NSRunLoop::mainRunLoop().performBlock(&commit) };
        }
        let started = std::time::Instant::now();
        backend.dispatch(Some(Duration::from_secs(5)))?;
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the commit woke the wait"
        );
        let mut committed = false;
        while let Some(event) = backend.next_event() {
            committed |= matches!(
                &event,
                Event::Ime { event: aegle_platform::ImeEvent::Update(update), .. }
                    if update.commit.as_deref() == Some("中文") && update.preedit.text.is_empty()
            );
        }
        assert!(committed, "the commit is reported");
        backend.configure_ime(first, None)?;
        // A wake from another thread ends a blocking wait.
        let wake = backend.wake_handle()?;
        settle(&mut backend)?;
        let thread = {
            let wake = wake.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                wake.wake();
            })
        };
        backend.dispatch(Some(Duration::from_secs(5)))?;
        thread.join().expect("the waking thread finishes");
        let mut woken = false;
        while let Some(event) = backend.next_event() {
            woken |= matches!(event, Event::Wake);
        }
        assert!(woken, "a wake is reported after a blocking dispatch");
        // Layers have no macOS equivalent, so the request fails as a capability.
        assert!(matches!(
            backend.create_window(WindowOptions {
                layer: Some(aegle_platform::LayerOptions {
                    layer: aegle_platform::Layer::Overlay,
                    anchor: aegle_platform::Anchor::NONE,
                    exclusive_zone: -1,
                    keyboard: aegle_platform::KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            backend.create_window(WindowOptions {
                size: PixelSize::default(),
                ..Default::default()
            }),
            Err(Error::InvalidSize)
        ));
        assert!(matches!(
            backend.create_window(WindowOptions {
                title: "nul\0inside",
                ..Default::default()
            }),
            Err(Error::InvalidString)
        ));
        let long = "x".repeat(4001);
        assert!(matches!(
            backend.create_window(WindowOptions {
                title: &long,
                ..Default::default()
            }),
            Err(Error::InvalidString)
        ));
        // A removed window and its queued events are gone for good.
        backend.remove_window(second)?;
        assert!(matches!(
            backend.window_info(second),
            Err(Error::InvalidWindow)
        ));
        assert!(matches!(
            backend.remove_window(second),
            Err(Error::InvalidWindow)
        ));
        assert!(matches!(
            backend.present(second, None, |_, _, _| Ok(())),
            Err(Error::InvalidWindow)
        ));
        assert!(backend.set_title(first, "renamed").is_ok());
        assert!(matches!(
            backend.set_title(first, "nul\0inside"),
            Err(Error::InvalidString)
        ));
        let kept = backend.window_info(first)?;
        assert!(kept.configured);
        drop(lease);
        backend.remove_window(first)?;
        Ok(())
    }

    /// Waits out the window's own AppKit events, such as its activation, so
    /// the next blocking wait ends only for what the scenario causes.
    fn settle(backend: &mut MacOs) -> Outcome {
        let quiet = Duration::from_millis(200);
        loop {
            let started = std::time::Instant::now();
            backend.dispatch(Some(quiet))?;
            while backend.next_event().is_some() {}
            if started.elapsed() >= quiet {
                return Ok(());
            }
        }
    }

    /// Posts a left press and release at the middle of `surface`'s window.
    fn press(surface: &aegle_platform_macos::WindowSurface) {
        use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView};
        use objc2_foundation::{MainThreadMarker, NSPoint};
        let RawWindowHandle::AppKit(handle) = surface.window_handle().unwrap().as_raw() else {
            unreachable!()
        };
        // SAFETY: the lease keeps the view alive on this, the main thread.
        let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
        let window = view.window().expect("a window shows the view");
        let size = view.bounds().size;
        let at = NSPoint::new(size.width / 2.0, size.height / 2.0);
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        for kind in [NSEventType::LeftMouseDown, NSEventType::LeftMouseUp] {
            let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            at,
            NSEventModifierFlags::empty(),
            0.0,
            window.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
        .expect("a mouse event");
            app.postEvent_atStart(&event, false);
        }
    }

    /// Screens list as outputs; popups attach to a parent, present, and must be
    /// removed before it.
    pub fn outputs_and_popups() -> Outcome {
        let mut backend = MacOs::connect()?;
        let outputs = backend.outputs()?;
        assert!(!outputs.is_empty(), "a GUI session has a screen");
        let primary = &outputs[0];
        assert_eq!(primary.position, aegle_platform::Point::new(0.0, 0.0));
        assert!(primary.size.width > 0 && primary.size.height > 0 && primary.scale >= 1.0);
        assert!(!primary.name.is_empty());
        let mut ids: Vec<_> = outputs.iter().map(|output| output.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), outputs.len());

        let parent = backend.create_window(WindowOptions {
            title: "Aegle popup parent",
            size: PixelSize {
                width: 320,
                height: 240,
            },
            ..Default::default()
        })?;
        assert!(backend.present(parent, None, |pixels, _, _| {
            pixels.fill(255);
            Ok(())
        })?);
        let popup = |anchor, grab| WindowOptions {
            size: PixelSize {
                width: 120,
                height: 80,
            },
            popup: Some(PopupOptions {
                parent,
                anchor,
                side: Side::Below,
                grab,
            }),
            ..Default::default()
        };
        let anchor = Rect::new(20.0, 20.0, 60.0, 24.0);
        assert!(matches!(
            backend.create_window(popup(Rect::new(20.0, 20.0, 0.0, 24.0), None)),
            Err(Error::InvalidRole(_))
        ));
        assert!(matches!(
            backend.create_window(WindowOptions {
                layer: Some(Default::default()),
                ..popup(anchor, None)
            }),
            Err(Error::InvalidRole(_))
        ));
        let menu = backend.create_window(popup(anchor, Some(SeatId(0))))?;
        let info = backend.window_info(menu)?;
        assert_eq!(
            info.size,
            PixelSize {
                width: 120,
                height: 80
            }
        );
        assert!(info.configured);
        assert!(backend.present(menu, None, |pixels, _, _| {
            pixels.fill(128);
            Ok(())
        })?);
        // A tooltip opened from the menu is a popup of a popup.
        let tip = backend.create_window(WindowOptions {
            popup: Some(PopupOptions {
                parent: menu,
                anchor: Rect::new(0.0, 0.0, 120.0, 20.0),
                side: Side::Right,
                grab: None,
            }),
            ..popup(anchor, None)
        })?;
        assert!(backend.present(tip, None, |_, _, _| Ok(()))?);
        for _ in 0..3 {
            backend.dispatch(Some(Duration::from_millis(30)))?;
        }
        let mut menu_focused = false;
        while let Some(event) = backend.next_event() {
            assert!(!matches!(event, Event::Error(_)), "{event:?}");
            if let Event::KeyboardFocus {
                window, focused, ..
            } = event
                && window == menu
            {
                menu_focused = focused;
            }
        }
        assert!(menu_focused, "a grabbing popup takes the keyboard");
        // A press in the tooltip stays inside the menu's chain; a press in the
        // parent dismisses the menu once, and the parent still receives it.
        press(&backend.window_surface(tip)?);
        backend.dispatch(Some(Duration::from_millis(30)))?;
        while let Some(event) = backend.next_event() {
            assert!(!matches!(event, Event::Close { .. }), "{event:?}");
        }
        let parent_surface = backend.window_surface(parent)?;
        press(&parent_surface);
        press(&parent_surface);
        let (mut closed, mut pressed) = (0, false);
        for _ in 0..3 {
            backend.dispatch(Some(Duration::from_millis(30)))?;
            while let Some(event) = backend.next_event() {
                match event {
                    Event::Close { window } => {
                        assert_eq!(window, menu, "only the grabbing popup is dismissed");
                        closed += 1;
                    }
                    Event::Pointer {
                        window,
                        kind: aegle_platform::PointerKind::Down { .. },
                        ..
                    } => pressed |= window == parent,
                    _ => {}
                }
            }
        }
        assert_eq!(closed, 1, "a dismissal is reported once");
        assert!(pressed, "the dismissing press is delivered");
        // Popups go before their parents, innermost first.
        assert!(matches!(
            backend.remove_window(parent),
            Err(Error::InvalidRole(_))
        ));
        assert!(matches!(
            backend.remove_window(menu),
            Err(Error::InvalidRole(_))
        ));
        backend.remove_window(tip)?;
        backend.remove_window(menu)?;
        backend.remove_window(parent)?;
        Ok(())
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateMouseEvent(
            source: *const std::ffi::c_void,
            kind: u32,
            at: objc2_foundation::NSPoint,
            button: u32,
        ) -> *mut std::ffi::c_void;
        fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
        fn CGEventSourceCreate(state: i32) -> *mut std::ffi::c_void;
        fn CFRelease(object: *mut std::ffi::c_void);
    }

    /// Posts a left-button mouse event of `kind` (`kCGEventLeftMouseDown` 1,
    /// `Up` 2, `MouseMoved` 5, `LeftMouseDragged` 6) at `at`, in the global
    /// top-left display space, through the window server.
    fn mouse(kind: u32, at: objc2_foundation::NSPoint) {
        // SAFETY: the source follows the HID system's state (1), so the
        // button stays down from press to release, as a drag session checks;
        // the source and event are released after posting to the HID tap (0).
        unsafe {
            let source = CGEventSourceCreate(1);
            assert!(!source.is_null());
            let event = CGEventCreateMouseEvent(source, kind, at, 0);
            assert!(!event.is_null());
            CGEventPost(0, event);
            CFRelease(event);
            CFRelease(source);
        }
    }

    /// The window showing `surface`'s view.
    fn ns_window(
        surface: &aegle_platform_macos::WindowSurface,
    ) -> objc2::rc::Retained<objc2_app_kit::NSWindow> {
        let RawWindowHandle::AppKit(handle) = surface.window_handle().unwrap().as_raw() else {
            unreachable!()
        };
        // SAFETY: the lease keeps the view alive on this, the main thread.
        let view: &objc2_app_kit::NSView = unsafe { handle.ns_view.cast().as_ref() };
        view.window().expect("a window shows the view")
    }

    /// The center of `window` in the global top-left display space.
    fn center(window: &objc2_app_kit::NSWindow) -> objc2_foundation::NSPoint {
        let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
        let primary = objc2_app_kit::NSScreen::screens(mtm)
            .firstObject()
            .map_or(0.0, |screen| screen.frame().size.height);
        let frame = window.frame();
        objc2_foundation::NSPoint::new(
            frame.origin.x + frame.size.width / 2.0,
            primary - (frame.origin.y + frame.size.height / 2.0),
        )
    }

    /// Text and then files dragged with the real pointer from one window to
    /// another: the target reports the drag, takes the host's answer and
    /// receives the drop.
    pub fn drag_between_windows() -> Outcome {
        use aegle_platform::{DragData, PointerKind};
        let mut backend = MacOs::connect()?;
        let options = WindowOptions {
            size: PixelSize {
                width: 300,
                height: 200,
            },
            ..Default::default()
        };
        let source = backend.create_window(WindowOptions {
            title: "Aegle drag source",
            ..options
        })?;
        let target = backend.create_window(WindowOptions {
            title: "Aegle drop target",
            ..options
        })?;
        for window in [source, target] {
            assert!(backend.present(window, None, |pixels, _, _| {
                pixels.fill(180);
                Ok(())
            })?);
        }
        let (source_surface, target_surface) = (
            backend.window_surface(source)?,
            backend.window_surface(target)?,
        );
        let (from, to) = (ns_window(&source_surface), ns_window(&target_surface));
        from.setFrameOrigin(objc2_foundation::NSPoint::new(120.0, 300.0));
        to.setFrameOrigin(objc2_foundation::NSPoint::new(520.0, 300.0));
        let pump = |backend: &mut MacOs, millis: u64, seen: &mut Vec<Event>| -> Outcome {
            let deadline = std::time::Instant::now() + Duration::from_millis(millis);
            while std::time::Instant::now() < deadline {
                backend.dispatch(Some(Duration::from_millis(10)))?;
                while let Some(event) = backend.next_event() {
                    if let Event::Drag { window, .. } = event {
                        backend.accept_drag(window, SeatId(0), window == target)?;
                    }
                    seen.push(event);
                }
            }
            Ok(())
        };
        let file = std::env::temp_dir().join(format!("aegle-drag-{}.txt", std::process::id()));
        std::fs::write(&file, "dragged")?;
        for data in [
            DragData::Text("拖放 drag".into()),
            DragData::Files(vec![file.canonicalize()?]),
        ] {
            let mut seen = Vec::new();
            pump(&mut backend, 300, &mut seen)?;
            let (start, end) = (center(&from), center(&to));
            mouse(5, start);
            mouse(1, start);
            seen.clear();
            pump(&mut backend, 200, &mut seen)?;
            assert!(
            seen.iter().any(|event| matches!(event, Event::Pointer { window, kind: PointerKind::Down { .. }, .. } if *window == source)),
            "the press reached the source: {seen:?}"
        );
            backend.start_drag(source, SeatId(0), &data)?;
            // AppKit tracks the drag in a loop nested in event delivery, so
            // the motion comes from another thread, as real input comes from
            // outside; this one keeps the loop running.
            let mover = std::thread::spawn(move || {
                for step in 1..=30 {
                    let t = f64::from(step) / 30.0;
                    mouse(
                        6,
                        objc2_foundation::NSPoint::new(
                            start.x + (end.x - start.x) * t,
                            start.y + (end.y - start.y) * t,
                        ),
                    );
                    std::thread::sleep(Duration::from_millis(15));
                }
                std::thread::sleep(Duration::from_millis(200));
                mouse(2, end);
            });
            while !mover.is_finished() {
                pump(&mut backend, 20, &mut seen)?;
            }
            mover.join().expect("the pointer thread finishes");
            pump(&mut backend, 500, &mut seen)?;
            assert!(
                seen.iter()
                    .any(|event| matches!(event, Event::Drag { window, .. } if *window == target)),
                "the target saw the drag: {seen:?}"
            );
            let dropped = seen.iter().find_map(|event| match event {
                Event::Drop {
                    window,
                    data,
                    position,
                    ..
                } if *window == target => Some((data.clone(), *position)),
                _ => None,
            });
            let (got, position) =
                dropped.ok_or_else(|| format!("no drop on the target: {seen:?}"))?;
            assert_eq!(got, data);
            // The session samples the pointer itself, a little behind the
            // posted motion; the drop lands inside the target.
            assert!(
                (0.0..300.0).contains(&position.x) && (0.0..200.0).contains(&position.y),
                "{position:?}"
            );
        }
        std::fs::remove_file(&file)?;
        drop((source_surface, target_surface));
        backend.remove_window(target)?;
        backend.remove_window(source)?;
        Ok(())
    }
}
