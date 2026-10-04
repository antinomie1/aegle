//! Run on a dedicated compositor: `cargo test -p aegle-platform-wayland --test native -- --ignored`.

use std::time::{Duration, Instant};

use aegle_platform_wayland::{
    Error, Event, ImeRequest, PixelSize, PresentError, Wayland, WindowId, WindowOptions,
};

fn redraw(platform: &mut Wayland, target: WindowId) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        platform.dispatch(Some(Duration::from_millis(20))).unwrap();
        while let Some(event) = platform.next_event() {
            match event {
                Event::Redraw { window } if window == target => return,
                Event::Error(error) => panic!("native input failure: {error}"),
                _ => {}
            }
        }
        assert!(
            Instant::now() < deadline,
            "compositor did not permit a frame"
        );
    }
}

fn present(platform: &mut Wayland, window: WindowId) {
    assert!(
        platform
            .present::<()>(window, |pixels, size| {
                assert_eq!(pixels.len(), size.width as usize * size.height as usize * 4);
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel.copy_from_slice(&[32, 48, 64, 255]);
                }
                Ok(())
            })
            .unwrap()
    );
}

#[test]
#[ignore = "requires a dedicated WAYLAND_DISPLAY compositor with visible frame callbacks"]
fn configured_frames_reuse_bounded_shm_and_idle_without_redrawing() {
    let mut platform = Wayland::connect().unwrap();
    let options = WindowOptions {
        title: "Aegle native lifecycle test",
        app_id: "org.aegle.native-test",
        size: PixelSize {
            width: 128,
            height: 96,
        },
        buffer_budget: 2 * 1024 * 1024,
    };
    let window = platform.create_window(options.clone()).unwrap();
    assert!(matches!(
        platform.create_window(WindowOptions {
            title: &"x".repeat(4001),
            ..options.clone()
        }),
        Err(Error::InvalidString)
    ));
    assert_eq!(platform.buffer_bytes(window).unwrap(), 0);
    assert!(
        !platform
            .present::<()>(window, |_, _| panic!("unconfigured draw"))
            .unwrap()
    );
    redraw(&mut platform, window);
    assert!(matches!(
        platform.present(window, |_, _| Err("incomplete frame")),
        Err(PresentError::Draw("incomplete frame"))
    ));
    platform.request_redraw(window).unwrap();
    redraw(&mut platform, window);
    present(&mut platform, window);
    assert!(
        !platform
            .present::<()>(window, |_, _| panic!("frame still pending"))
            .unwrap()
    );
    for _ in 0..4 {
        platform.request_redraw(window).unwrap();
        redraw(&mut platform, window);
        present(&mut platform, window);
    }
    let size = platform.window_info(window).unwrap().buffer_size().unwrap();
    let single_mapping = (size.width as usize * size.height as usize * 4 + 63) & !63;
    assert!(
        (single_mapping..=2 * single_mapping).contains(&platform.buffer_bytes(window).unwrap())
    );

    // Native configure/focus/frame events may still arrive. Once drained, a
    // finite wait must sleep instead of synthesizing more redraw notifications.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let start = Instant::now();
        platform.dispatch(Some(Duration::from_millis(50))).unwrap();
        let waited = start.elapsed();
        while let Some(event) = platform.next_event() {
            assert!(!matches!(event, Event::Redraw { .. }));
            if let Event::Error(error) = event {
                panic!("native input failure: {error}");
            }
        }
        if waited >= Duration::from_millis(40) {
            break;
        }
        assert!(Instant::now() < deadline, "native loop did not become idle");
    }
    let invalid = ImeRequest {
        surrounding: Some("汉".into()),
        cursor: 1,
        ..ImeRequest::default()
    };
    assert!(matches!(
        platform.configure_ime(window, Some(invalid)),
        Err(Error::InvalidIme(_))
    ));

    let constrained = platform
        .create_window(WindowOptions {
            buffer_budget: 0,
            ..options.clone()
        })
        .unwrap();
    redraw(&mut platform, constrained);
    assert!(matches!(
        platform.present::<()>(constrained, |_, _| panic!("budget rejected before drawing")),
        Err(PresentError::Platform(Error::BufferBudget {
            budget: 0,
            ..
        }))
    ));
    assert_eq!(platform.buffer_bytes(constrained).unwrap(), 0);
    platform.remove_window(constrained).unwrap();
    platform.remove_window(window).unwrap();
    assert!(matches!(
        platform.request_redraw(window),
        Err(Error::InvalidWindow)
    ));
    // A new extent creates fresh bounded storage and cannot reuse a destroyed ID.
    let replacement = platform
        .create_window(WindowOptions {
            size: PixelSize {
                width: 256,
                height: 192,
            },
            ..options
        })
        .unwrap();
    assert_ne!(window, replacement);
    redraw(&mut platform, replacement);
    present(&mut platform, replacement);
    platform.remove_window(replacement).unwrap();
    platform.dispatch(Some(Duration::ZERO)).unwrap();
    assert!(platform.next_event().is_none());
}
