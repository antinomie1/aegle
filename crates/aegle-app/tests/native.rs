//! Native application lifecycle, only in a dedicated test desktop.
#![cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]

use aegle_app::{App, AppOptions, Result, TextSystem, UiError, WindowOptions};
use aegle_text::{Blob, GenericFamily};
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn windows_callbacks_and_deferred_actions_share_one_native_loop() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut fonts = TextSystem::new();
    let family = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?[0]
        .0;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, [family].into_iter());
    let mut settings = AppOptions::default();
    if std::env::var_os("AEGLE_TEST_VULKAN").is_some() {
        settings.renderer = aegle_app::RendererBackend::Vulkan;
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
        assert!(!stale.is_alive());
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
    assert!(!label.is_alive());
    Ok(())
}
