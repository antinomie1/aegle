//! Native lifecycle contract, intentionally opt-in on a macOS GUI session:
//! `cargo test -p aegle-platform-macos --test native -- --ignored`.
//!
//! AppKit runs on the main thread only, which libtest never gives a test, so
//! this target has its own `main` and runs its scenarios in order.
#![allow(unsafe_code)] // Borrowing the leased view to post events to it.
use std::error::Error as StdError;

type Outcome = Result<(), Box<dyn StdError>>;

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
        backend.configure_ime(first, None)?;
        // A wake from another thread ends a blocking wait.
        let wake = backend.wake_handle()?;
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
}
