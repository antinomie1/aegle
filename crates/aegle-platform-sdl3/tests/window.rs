//! One window on the session's own video driver, drawn and presented once.
//!
//! Needs a display session (Wayland or X11) and is therefore opt-in:
//! `cargo test -p aegle-platform-sdl3 --test window -- --ignored`.

use std::time::{Duration, Instant};

use aegle_platform::{Event, PixelRect, Platform, Region, WindowOptions};
use aegle_platform_sdl3::Sdl3;

/// The next contract event, dispatching while the queue is empty.
fn next(platform: &mut Sdl3, wanted: &str) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(event) = platform.next_event() {
            return event;
        }
        assert!(Instant::now() < deadline, "no {wanted} event");
        platform.dispatch(Some(Duration::from_millis(50))).unwrap();
    }
}

#[test]
#[ignore = "requires a display session"]
fn a_window_presents_on_the_session_driver() {
    let mut platform = Sdl3::connect().unwrap();
    let window = platform
        .create_window(WindowOptions {
            title: "aegle sdl3 test",
            app_id: "org.aegle.test",
            size: aegle_platform::PixelSize {
                width: 240,
                height: 160,
            },
            buffer_budget: usize::MAX,
            layer: None,
        })
        .unwrap();
    let info = loop {
        match next(&mut platform, "configure") {
            Event::Configure { window: id, info } if id == window => break info,
            _ => continue,
        }
    };
    assert_eq!(info.size.width, 240);
    assert!(info.configured);
    assert!(info.scale > 0.0);

    // A new window is dirty, so its first frame arrives without an explicit
    // redraw request.
    loop {
        match next(&mut platform, "redraw") {
            Event::Redraw { window: id } if id == window => break,
            _ => continue,
        }
    }
    let mut damage = Region::default();
    damage.add(PixelRect::full(info.buffer.width, info.buffer.height));
    let size = info.buffer;
    let drawn = platform
        .present(window, Some(damage), |pixels, size, _| {
            // Opaque dark blue, premultiplied BGRA8.
            let mut row = Vec::with_capacity(size.width as usize * 4);
            for _ in 0..size.width {
                row.extend_from_slice(&[90, 40, 20, 255]);
            }
            for line in pixels.chunks_exact_mut(row.len()) {
                line.copy_from_slice(&row);
            }
            Ok(())
        })
        .unwrap();
    assert!(drawn, "the frame reached the compositor");
    assert_eq!(platform.window_info(window).unwrap().buffer, size);
    platform.remove_window(window).unwrap();
}
