//! Native lifecycle contract, intentionally opt-in on a macOS GUI session.
#![cfg(target_os = "macos")]
use aegle_platform::{
    Error, Event, ImeRequest, PixelSize, Platform, Rect, SurfacePlatform, WindowOptions,
};
use aegle_platform_macos::MacOs;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::Duration;

/// Presses nothing and takes the window down again: the contract's window,
/// presentation, wake, IME and error paths.
#[test]
#[ignore = "creates native windows; run on a macOS GUI session"]
fn native_lifecycle_pixels_and_owned_surface() -> Result<(), Box<dyn std::error::Error>> {
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
