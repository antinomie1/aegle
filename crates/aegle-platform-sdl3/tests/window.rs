//! Windows and popups on the session's own video driver.
//!
//! Needs a display session (Wayland or X11) and is therefore opt-in. SDL's
//! video state is process-wide, so the tests run one at a time:
//! `cargo test -p aegle-platform-sdl3 --test window -- --ignored --test-threads=1`.

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
            popup: None,
            transparent: false,
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

#[test]
#[ignore = "requires a display session"]
fn a_popup_opens_beside_its_parent_and_outlives_nothing() {
    let mut platform = Sdl3::connect().unwrap();
    assert!(!platform.outputs().unwrap().is_empty());
    let options = WindowOptions {
        size: aegle_platform::PixelSize {
            width: 240,
            height: 160,
        },
        ..Default::default()
    };
    let parent = platform.create_window(options).unwrap();
    let menu = platform
        .create_window(WindowOptions {
            size: aegle_platform::PixelSize {
                width: 100,
                height: 80,
            },
            popup: Some(aegle_platform::PopupOptions {
                parent,
                anchor: aegle_platform::Rect::new(20.0, 20.0, 60.0, 24.0),
                placement: aegle_platform::Placement::Below,
                grab: None,
            }),
            ..options
        })
        .unwrap();
    let info = loop {
        match next(&mut platform, "popup configure") {
            Event::Configure { window, info } if window == menu && info.configured => break info,
            _ => continue,
        }
    };
    assert_eq!((info.size.width, info.size.height), (100, 80));
    assert!(matches!(
        platform.remove_window(parent),
        Err(aegle_platform::Error::InvalidRole(_))
    ));
    platform.remove_window(menu).unwrap();
    platform.remove_window(parent).unwrap();
}
