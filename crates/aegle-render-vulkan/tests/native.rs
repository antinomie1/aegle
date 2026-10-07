//! Requires an explicitly isolated compositor; never opens the user's desktop.
#![cfg(all(target_os = "linux", feature = "window"))]
use aegle_platform_wayland::{Event, Wayland, WindowOptions};
use aegle_render_vulkan::{Error, Options, WindowRenderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_types::PixelRect;
use std::time::{Duration, Instant};

#[test]
#[allow(unsafe_code)]
#[ignore = "requires AEGLE_TEST_COMPOSITOR=private and an isolated Wayland compositor/Vulkan ICD"]
fn native_present_resize_suspend_and_owned_close() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut platform = Wayland::connect()?;
    let id = platform.create_window(WindowOptions {
        title: "Aegle native Vulkan lifecycle test",
        ..Default::default()
    })?;
    // SAFETY: This owned lease keeps the window and display alive past Renderer.
    let mut renderer =
        unsafe { WindowRenderer::new(platform.window_surface(id)?, Options::default())? };
    assert!(renderer.begin_frame(0, 0, Color::WHITE)?.is_none());
    eprintln!("Native Vulkan device: {}", renderer.device_name());
    let mut scene = SceneBuilder::new();
    scene.fill(
        RoundedRect::new(Rect::new(20.0, 20.0, 200.0, 100.0), 15.0)?,
        Color::rgb(0, 80, 230),
    )?;
    let scene = scene.finish()?;
    // More primitives than one submission holds: the swapchain image is
    // acquired by the first part and presented after the last.
    let mut large = SceneBuilder::new();
    for index in 0..20_000_u16 {
        let (x, y) = (f32::from(index % 128), f32::from(index / 128));
        large.fill(
            RoundedRect::new(Rect::new(x, y, 1.0, 1.0), 0.0)?,
            Color::rgb(230, 40, 0),
        )?;
    }
    let large = large.finish()?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut frames = 0;
    while Instant::now() < deadline {
        while let Some(event) = platform.next_event() {
            if !matches!(event, Event::Redraw { .. }) {
                continue;
            }
            let presented = platform.present_external(id, |size| -> Result<bool, Error> {
                if frames == 1 {
                    let _ = renderer.begin_frame(64, 48, Color::WHITE)?;
                    assert_eq!(renderer.extent(), [64, 48]);
                    let _ = renderer.begin_frame(size.width, size.height, Color::WHITE)?;
                    let mut bad = renderer
                        .begin_frame(size.width, size.height, Color::WHITE)?
                        .unwrap();
                    assert!(
                        bad.draw_clipped(
                            &scene,
                            Affine::IDENTITY,
                            Some(Rect::new(f32::NAN, 0.0, 1.0, 1.0))
                        )
                        .is_err()
                    );
                    assert!(matches!(bad.finish(), Err(Error::FrameFailed)));
                }
                let Some(mut frame) =
                    renderer.begin_frame(size.width, size.height, Color::WHITE)?
                else {
                    return Ok(false);
                };
                if frames == 2 {
                    frame.draw(&large, Affine::IDENTITY)?;
                }
                frame.draw(&scene, Affine::IDENTITY)?;
                // Later frames name changed regions, partly beyond the extent.
                if frames > 1 {
                    let rect = |x, y, width, height| PixelRect {
                        x,
                        y,
                        width,
                        height,
                    };
                    frame.set_damage(&[rect(1, 2, 10, 5), rect(30, 30, 10_000, 4)]);
                }
                frame.finish()?;
                Ok(true)
            })?;
            if !presented {
                continue;
            }
            frames += 1;
            let stats = renderer.stats();
            assert!(stats.swapchain_bytes > 0);
            assert!(stats.device_bytes <= Options::default().memory_budget);
            assert_eq!(platform.buffer_bytes(id)?, 0);
            if frames == 3 {
                assert!(renderer.begin_frame(0, 0, Color::WHITE)?.is_none());
                assert_eq!(renderer.extent(), [0; 2]);
            }
            if frames == 4 {
                renderer.release_images()?;
                assert_eq!(renderer.stats().device_bytes, 0);
                assert_eq!(renderer.stats().swapchain_bytes, 0);
            }
            if frames == 6 {
                platform.remove_window(id)?;
                drop(platform);
                renderer.wait()?;
                drop(renderer);
                return Ok(());
            }
            platform.request_redraw(id)?;
        }
        platform.dispatch(Some(Duration::from_millis(100)))?;
    }
    Err("native presentation timed out".into())
}

#[test]
#[allow(unsafe_code)]
#[ignore = "requires AEGLE_TEST_COMPOSITOR=private and an isolated Wayland compositor/Vulkan ICD"]
fn two_windows_present_through_one_shared_device() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut platform = Wayland::connect()?;
    let ids = [0, 1].map(|n| {
        platform
            .create_window(WindowOptions {
                title: if n == 0 { "Shared A" } else { "Shared B" },
                ..Default::default()
            })
            .unwrap()
    });
    // SAFETY: The owned leases keep both windows and the display alive past the renderers.
    let first =
        unsafe { WindowRenderer::new(platform.window_surface(ids[0])?, Options::default())? };
    let second = unsafe {
        WindowRenderer::with_device(
            platform.window_surface(ids[1])?,
            Options::default(),
            &first.shared_device(),
        )?
    };
    assert_eq!(first.device_name(), second.device_name());
    let mut renderers = [Some(first), Some(second)];
    let mut scene = SceneBuilder::new();
    scene.fill(
        RoundedRect::new(Rect::new(10.0, 10.0, 80.0, 40.0), 6.0)?,
        Color::rgb(200, 40, 40),
    )?;
    let scene = scene.finish()?;
    let (mut presented, mut dropped) = ([0u32; 2], false);
    for id in ids {
        platform.request_redraw(id)?;
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline && presented[1] < 4 {
        while let Some(event) = platform.next_event() {
            let Event::Redraw { window } = event else {
                continue;
            };
            let index = ids.iter().position(|id| *id == window).unwrap();
            let Some(renderer) = renderers[index].as_mut() else {
                continue;
            };
            let done = platform.present_external(window, |size| -> Result<bool, Error> {
                let Some(mut frame) =
                    renderer.begin_frame(size.width, size.height, Color::WHITE)?
                else {
                    return Ok(false);
                };
                frame.draw(&scene, Affine::IDENTITY)?;
                frame.finish()?;
                Ok(true)
            })?;
            presented[index] += u32::from(done);
            platform.request_redraw(window)?;
            // The creating renderer may go first; the device lives on in the other.
            if !dropped && presented[0] >= 1 && presented[1] >= 1 {
                renderers[0] = None;
                dropped = true;
            }
        }
        platform.dispatch(Some(Duration::from_millis(20)))?;
    }
    assert!(dropped && presented[1] >= 4, "{presented:?}");
    renderers[1].as_mut().unwrap().wait()?;
    Ok(())
}
