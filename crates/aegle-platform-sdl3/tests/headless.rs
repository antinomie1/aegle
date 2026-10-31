//! Window lifecycle, presentation budget and capabilities on SDL's dummy video
//! driver, which needs no display server.
//!
//! SDL's video subsystem is process-wide and its driver is chosen when it
//! initializes, so these tests take turns on a mutex and set the driver before
//! the first connection.
#![allow(unsafe_code)]

use std::{
    sync::{Mutex, MutexGuard, Once},
    time::Duration,
};

use aegle_platform::{
    Cursor, DragData, Error, Event, Layer, LayerOptions, PixelRect, PixelSize, Platform, Region,
    SeatId, WindowId, WindowOptions,
};
use aegle_platform_sdl3::Sdl3;

/// Serializes access to SDL's process-wide video state.
static SERIAL: Mutex<()> = Mutex::new(());

fn platform() -> (MutexGuard<'static, ()>, Sdl3) {
    let guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    static DRIVER: Once = Once::new();
    DRIVER.call_once(|| {
        // SAFETY: no other thread of this test binary reads the environment
        // before the serialized first connection below.
        unsafe { std::env::set_var("SDL_VIDEODRIVER", "dummy") };
    });
    let platform = Sdl3::connect().unwrap();
    (guard, platform)
}

fn options(width: u32, height: u32) -> WindowOptions<'static> {
    WindowOptions {
        title: "aegle test",
        app_id: "org.aegle.test",
        size: PixelSize { width, height },
        buffer_budget: usize::MAX,
        layer: None,
    }
}

/// Dispatches until `wanted` accepts an event, or fails the test.
fn wait_for(platform: &mut Sdl3, mut wanted: impl FnMut(&Event) -> bool) -> Event {
    for _ in 0..100 {
        platform.dispatch(Some(Duration::from_millis(50))).unwrap();
        while let Some(event) = platform.next_event() {
            if wanted(&event) {
                return event;
            }
        }
    }
    panic!("no matching event");
}

#[test]
fn a_window_configures_and_presents_its_whole_buffer() {
    let (_guard, mut platform) = platform();
    let window = platform.create_window(options(64, 48)).unwrap();
    let event = wait_for(
        &mut platform,
        |event| matches!(event, Event::Configure { window: id, .. } if *id == window),
    );
    let Event::Configure { info, .. } = event else {
        unreachable!()
    };
    assert_eq!(
        info.size,
        PixelSize {
            width: 64,
            height: 48
        }
    );
    assert!(info.configured);
    assert!(info.buffer.width >= info.size.width);
    assert!(info.scale >= 1.0);

    platform.request_redraw(window).unwrap();
    wait_for(
        &mut platform,
        |event| matches!(event, Event::Redraw { window: id } if *id == window),
    );
    let damage = platform
        .window_info(window)
        .map(|info| {
            let mut region = Region::default();
            region.add(PixelRect::full(info.buffer.width, info.buffer.height));
            region
        })
        .unwrap();
    let mut drawn = None;
    let presented = platform
        .present(window, Some(damage), |pixels, size, region| {
            drawn = Some((pixels.len(), size, region.rects().len()));
            pixels.fill(0x80);
            Ok(())
        })
        .unwrap();
    assert!(presented, "a configured window presents");
    let (length, size, rects) = drawn.expect("the drawing callback ran");
    assert_eq!(length as u32, size.width * size.height * 4);
    assert_eq!(rects, 1);
    // A presented frame leaves no redraw queued until the host changes again.
    platform.dispatch(Some(Duration::ZERO)).unwrap();
    assert!(
        !std::iter::from_fn(|| platform.next_event())
            .any(|event| matches!(event, Event::Redraw { .. }))
    );
}

#[test]
fn a_presentation_budget_is_enforced() {
    let (_guard, mut platform) = platform();
    let mut options = options(32, 32);
    options.buffer_budget = 64;
    let window = platform.create_window(options).unwrap();
    platform.request_redraw(window).unwrap();
    wait_for(
        &mut platform,
        |event| matches!(event, Event::Redraw { window: id } if *id == window),
    );
    let error = platform
        .present(window, None, |_, _, _| Ok(()))
        .expect_err("64 bytes cannot hold a 32x32 buffer");
    assert!(
        matches!(error, Error::BufferBudget { required, budget } if required == 32 * 32 * 4 && budget == 64),
        "{error}"
    );
}

#[test]
fn layers_are_unsupported() {
    let (_guard, mut platform) = platform();
    let mut options = options(64, 32);
    options.layer = Some(LayerOptions {
        layer: Layer::Top,
        anchor: aegle_platform::Anchor::TOP,
        exclusive_zone: 0,
        keyboard: aegle_platform::KeyboardInteractivity::None,
    });
    assert!(matches!(
        platform.create_window(options),
        Err(Error::Unsupported("layer surfaces"))
    ));
}

#[test]
fn titles_and_sizes_are_validated() {
    let (_guard, mut platform) = platform();
    assert!(matches!(
        platform.create_window(options(0, 48)),
        Err(Error::InvalidSize)
    ));
    let mut invalid = options(64, 48);
    invalid.title = "with\0nul";
    assert!(matches!(
        platform.create_window(invalid),
        Err(Error::InvalidString)
    ));
    let window = platform.create_window(options(64, 48)).unwrap();
    platform.set_title(window, "aegle test").unwrap();
    assert!(matches!(
        platform.set_title(window, "with\0nul"),
        Err(Error::InvalidString)
    ));
    platform.set_cursor(window, Cursor::Text).unwrap();
}

#[test]
fn removed_windows_are_invalid() {
    let (_guard, mut platform) = platform();
    let window = platform.create_window(options(64, 48)).unwrap();
    platform.remove_window(window).unwrap();
    assert!(matches!(
        platform.window_info(window),
        Err(Error::InvalidWindow)
    ));
    assert!(matches!(
        platform.request_redraw(window),
        Err(Error::InvalidWindow)
    ));
    assert!(matches!(
        platform.remove_window(window),
        Err(Error::InvalidWindow)
    ));
    assert!(matches!(
        platform.window_info(WindowId(u64::MAX)),
        Err(Error::InvalidWindow)
    ));
}

#[test]
fn the_dummy_driver_has_no_composition_and_no_drag_source() {
    let (_guard, mut platform) = platform();
    assert!(!platform.ime_available());
    let window = platform.create_window(options(64, 48)).unwrap();
    let request = aegle_platform::ImeRequest::default();
    assert!(matches!(
        platform.configure_ime(window, Some(request)),
        Err(Error::ImeUnavailable)
    ));
    platform.configure_ime(window, None).unwrap();
    assert!(matches!(
        platform.start_drag(window, SeatId(0), &DragData::Text("x".into())),
        Err(Error::Unsupported(_))
    ));
    platform.accept_drag(window, SeatId(0), true).unwrap();
}

#[test]
fn the_clipboard_round_trips_through_an_event() {
    let (_guard, mut platform) = platform();
    let window = platform.create_window(options(64, 48)).unwrap();
    platform
        .set_clipboard(window, SeatId(0), "剪贴板 text")
        .unwrap();
    platform.request_clipboard(window, SeatId(0)).unwrap();
    let event = wait_for(&mut platform, |event| {
        matches!(event, Event::Clipboard { .. })
    });
    let Event::Clipboard {
        window: id,
        seat,
        text,
    } = event
    else {
        panic!("not a clipboard event: {event:?}")
    };
    assert_eq!((id, seat), (window, SeatId(0)));
    assert_eq!(text, "剪贴板 text");
}

#[test]
fn a_wake_handle_wakes_the_dispatch() {
    let (_guard, mut platform) = platform();
    let wake = platform.wake_handle().unwrap();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        wake.wake();
    });
    let started = std::time::Instant::now();
    let event = wait_for(&mut platform, |event| matches!(event, Event::Wake));
    assert!(matches!(event, Event::Wake));
    assert!(started.elapsed() < Duration::from_secs(5));
    thread.join().unwrap();
}

#[test]
fn animation_frames_are_paced_by_the_display() {
    let (_guard, mut platform) = platform();
    let window = platform.create_window(options(32, 32)).unwrap();
    wait_for(
        &mut platform,
        |event| matches!(event, Event::Configure { window: id, .. } if *id == window),
    );
    // Two redraws requested back to back: the second waits for the frame after
    // the first instead of drawing twice in the same instant.
    platform.request_redraw(window).unwrap();
    wait_for(
        &mut platform,
        |event| matches!(event, Event::Redraw { window: id } if *id == window),
    );
    platform
        .present(window, None, |pixels, _, _| {
            pixels.fill(0);
            Ok(())
        })
        .unwrap();
    platform.request_redraw(window).unwrap();
    platform.dispatch(Some(Duration::ZERO)).unwrap();
    let mut early = 0;
    while let Some(event) = platform.next_event() {
        if matches!(event, Event::Redraw { .. }) {
            early += 1;
        }
    }
    assert_eq!(early, 0, "the next frame waits for the display frame");
    wait_for(
        &mut platform,
        |event| matches!(event, Event::Redraw { window: id } if *id == window),
    );
}
