//! Requires an explicitly isolated compositor; never opens the user's desktop.
#![cfg(all(target_os = "linux", feature = "window"))]
use aegle_platform_wayland::{Event, Wayland, WindowOptions};
use aegle_render_vulkan::{Error, Options, WindowRenderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
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
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut frames = 0;
    while Instant::now() < deadline {
        while let Some(event) = platform.next_event() {
            if !matches!(event, Event::Redraw { .. }) {
                continue;
            }
            let presented = platform.present_external(id, |size| -> Result<bool, Error> {
                if frames == 1 {
                    drop(renderer.begin_frame(64, 48, Color::WHITE)?);
                    assert_eq!(renderer.extent(), [64, 48]);
                    drop(renderer.begin_frame(size.width, size.height, Color::WHITE)?);
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
                frame.draw(&scene, Affine::IDENTITY)?;
                frame.finish()?;
                Ok(true)
            })?;
            if !presented {
                continue;
            }
            frames += 1;
            let stats = renderer.stats();
            assert!(stats.swapchain_bytes > 0);
            assert!(stats.device_bytes + stats.swapchain_bytes <= Options::default().memory_budget);
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
