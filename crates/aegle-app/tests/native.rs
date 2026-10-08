//! Native application lifecycle, only in a dedicated test desktop.
#![cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]

use aegle_app::{App, AppOptions, WindowOptions};
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{Result, TextSystem, UiError};
use aegle_widgets::Widgets;
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn test_fonts() -> Result<TextSystem> {
    let mut fonts = TextSystem::new();
    let family = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?[0]
        .0;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, [family].into_iter());
    Ok(fonts)
}

#[test]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn windows_callbacks_and_deferred_actions_share_one_native_loop() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let fonts = test_fonts()?;
    let mut settings = AppOptions::default();
    if std::env::var_os("AEGLE_TEST_VULKAN").is_some() {
        settings.renderer = aegle_app::RendererBackend::Vulkan;
    }
    if std::env::var_os("AEGLE_TEST_WGPU").is_some() {
        settings.renderer = aegle_app::RendererBackend::Wgpu;
    }
    let app = Rc::new(App::with_fonts(fonts, settings)?);
    let options = WindowOptions {
        width: 240,
        height: 160,
        ..Default::default()
    };
    let first = app.window_with_options("First", options)?;
    let second = app.window_with_options("Second", options)?;
    let label = first.text("Hello, 世界")?;
    let start = first.button("Start")?;
    let finish = second.button("Finish")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.dispatch(Some(Duration::from_millis(20)))?;
    }
    assert!(label.bounds()?.size.width > 0.0);
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    finish.on_click(move |_| {
        second.close()?;
        done.set(true);
        Ok(())
    })?;
    let nested = Rc::downgrade(&app);
    let stale = label.clone();
    start.on_click(move |_| {
        let nested = nested.upgrade().unwrap();
        let error = nested.dispatch(Some(Duration::ZERO)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UiError>(),
            Some(&UiError::ReentrantAccess)
        );
        first.close()?;
        assert!(!stale.is_alive()?);
        assert!(stale.set_text("dead").is_err());
        finish.activate()
    })?;
    start.activate()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.dispatch(Some(Duration::from_millis(20)))? {
        assert!(
            Instant::now() < deadline,
            "queued application action was stranded"
        );
    }
    assert!(completed.get());
    assert!(!label.is_alive()?);
    Ok(())
}

/// Stripes under a translucent panel that blurs them and a half-opaque
/// group, presented for `AEGLE_TEST_HOLD_MS` (default 600) so a screenshot
/// can compare backends. A backend that cannot blur its swapchain fails the
/// frame instead of skipping the blur.
#[test]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn layers_draw_into_native_windows() -> Result {
    use aegle_ui::{Color, Point};
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut settings = AppOptions::default();
    if std::env::var_os("AEGLE_TEST_VULKAN").is_some() {
        settings.renderer = aegle_app::RendererBackend::Vulkan;
    }
    if std::env::var_os("AEGLE_TEST_WGPU").is_some() {
        settings.renderer = aegle_app::RendererBackend::Wgpu;
    }
    let app = App::with_fonts(test_fonts()?, settings)?;
    let options = WindowOptions {
        width: 240,
        height: 160,
        ..Default::default()
    };
    let window = app.window_with_options("Layers", options)?;
    window.set_background(Color::WHITE)?;
    let stripes = window.row()?;
    stripes.set_gap(6.0)?;
    for _ in 0..12 {
        let stripe = stripes.column()?;
        stripe.set_size(8.0, 120.0)?;
        stripe.set_background(Color::rgb(40, 80, 220))?;
    }
    let panel = window.column()?;
    panel.set_size(120.0, 60.0)?;
    panel.set_radius(10.0)?;
    panel.set_offset(Point::new(40.0, -110.0))?;
    panel.set_background(Color::rgba(255, 255, 255, 90))?;
    panel.set_backdrop_blur(4.0)?;
    let group = panel.column()?;
    group.set_opacity(0.5)?;
    for color in [Color::rgb(220, 40, 40), Color::rgb(40, 160, 60)] {
        let square = group.column()?;
        square.set_size(24.0, 24.0)?;
        square.set_background(color)?;
    }
    let hold = std::env::var("AEGLE_TEST_HOLD_MS").map_or(600, |ms| ms.parse().unwrap());
    let deadline = Instant::now() + Duration::from_millis(hold);
    while Instant::now() < deadline {
        app.dispatch(Some(Duration::from_millis(20)))?;
    }
    assert!(panel.bounds()?.size.width > 0.0);
    Ok(())
}

#[test]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn proxy_messages_from_threads_reach_the_ui_thread_in_order() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let app = App::with_fonts(test_fonts()?, AppOptions::default())?;
    let window = app.window("Proxy")?;
    let label = window.text("waiting")?;
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    let closing = window.clone();
    let proxy = app.proxy(move |n: u32| {
        log.borrow_mut().push(n);
        label.set_text(&format!("got {n}"))?;
        if n == 3 {
            closing.close()?;
        }
        Ok(())
    })?;
    // Sent from another thread while the loop sleeps in the compositor wait.
    let sender = proxy.clone();
    let worker = std::thread::spawn(move || {
        for n in 1..=3 {
            std::thread::sleep(Duration::from_millis(50));
            sender.send(n).unwrap();
        }
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.dispatch(Some(Duration::from_secs(1)))? {
        assert!(Instant::now() < deadline, "proxy wake was lost");
    }
    worker.join().unwrap();
    assert_eq!(*seen.borrow(), [1, 2, 3]);
    drop(app);
    assert_eq!(
        proxy.send(4),
        Err(4),
        "a closed application rejects messages"
    );
    Ok(())
}

#[test]
#[cfg(feature = "software")]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn software_windows_reject_a_translucent_background() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut theme = aegle_ui::Theme::light();
    theme.background = aegle_types::Color::rgba(255, 255, 255, 128);
    let options = AppOptions {
        theme,
        dark_theme: None,
        high_contrast_theme: None,
        ..Default::default()
    };
    let app = App::with_fonts(test_fonts()?, options)?;
    let _window = app.window("Translucent")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let error = loop {
        match app.dispatch(Some(Duration::from_millis(20))) {
            Err(error) => break error,
            Ok(_) => assert!(
                Instant::now() < deadline,
                "a translucent frame was presented"
            ),
        }
    };
    assert_eq!(
        error.to_string(),
        "software windows require an opaque background"
    );
    Ok(())
}
