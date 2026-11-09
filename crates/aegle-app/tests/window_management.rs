//! Window management through the host on a real macOS window server: the
//! options a window opens with, its mode and size methods, and a close
//! request the window's handler declines once. Opt-in on a GUI session:
//! `cargo test -p aegle-app --features macos,tiny-skia --test window_management -- --ignored`.
//!
//! AppKit runs on the main thread only, which libtest never gives a test, so
//! this target has its own `main`.

#[cfg(all(target_os = "macos", feature = "macos"))]
fn main() -> aegle_ui::Result {
    if !std::env::args().any(|arg| arg == "--ignored" || arg == "--include-ignored") {
        println!("window_management: skipped; pass --ignored on a macOS GUI session");
        return Ok(());
    }
    macos::run()?;
    println!("window_management: options, modes and close requests ... ok");
    Ok(())
}

#[cfg(not(all(target_os = "macos", feature = "macos")))]
fn main() {}

#[cfg(all(target_os = "macos", feature = "macos"))]
mod macos {
    use aegle_app::{App, AppOptions, WindowMode, WindowOptions};
    use aegle_text::GenericFamily;
    use aegle_ui::{Point, Result, TextSystem};
    use aegle_widgets::Widgets;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{MainThreadMarker, NSString};
    use std::{
        cell::Cell,
        rc::Rc,
        sync::Arc,
        time::{Duration, Instant},
    };

    fn fonts() -> Result<TextSystem> {
        let mut fonts = TextSystem::new();
        let family = fonts.register_fonts(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        ))?[0];
        fonts.set_generic_families(GenericFamily::SansSerif, [family]);
        Ok(fonts)
    }

    /// Dispatches until `done` holds, for at most five seconds.
    fn until(app: &App, what: &str, done: impl Fn() -> Result<bool>) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done()? {
            if Instant::now() > deadline {
                return Err(format!("timed out waiting for {what}").into());
            }
            app.dispatch(Some(Duration::from_millis(20)))?;
        }
        Ok(())
    }

    /// Asks the AppKit window titled `title` to close, as its close button does.
    fn press_close(title: &str) {
        let mtm = MainThreadMarker::new().expect("the test runs on the main thread");
        let title = NSString::from_str(title);
        let windows = NSApplication::sharedApplication(mtm).windows();
        let window = windows
            .iter()
            .find(|window| window.title() == title)
            .expect("the window is on screen");
        window.performClose(None);
    }

    pub fn run() -> Result<()> {
        let app = App::with_fonts(fonts()?, AppOptions::default())?;
        let title = "Aegle host window management";
        let window = app.window_with_options(
            title,
            WindowOptions {
                width: 360,
                height: 240,
                min_size: Some((320, 200)),
                position: Some(Point::new(80.0, 120.0)),
                mode: WindowMode::Maximized,
                ..Default::default()
            },
        )?;
        window.text("你好");
        until(&app, "maximized", || {
            Ok(window.mode()? == WindowMode::Maximized)
        })?;
        window.set_mode(WindowMode::Normal)?;
        until(
            &app,
            "restored",
            || Ok(window.mode()? == WindowMode::Normal),
        )?;
        // The window returns to the size and position it opened with.
        assert_eq!(window.size()?, (360, 240));
        assert_eq!(window.position()?, Point::new(80.0, 120.0));
        // The minimum holds against a smaller request.
        window.set_size(100, 100)?;
        assert_eq!(window.size()?, (320, 200));
        assert!(
            window
                .set_size_limits(Some((400, 400)), Some((300, 300)))
                .is_err()
        );

        let asked = Rc::new(Cell::new(0));
        let count = asked.clone();
        window.on_close(move || {
            count.set(count.get() + 1);
            Ok(count.get() > 1)
        })?;
        // The first request is declined: the window and its controls stay.
        press_close(title);
        until(&app, "the first close request", || Ok(asked.get() == 1))?;
        assert!(window.is_alive());
        assert_eq!(window.size()?, (320, 200));
        // The second is accepted, and the last window closing ends the loop.
        press_close(title);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.dispatch(Some(Duration::from_millis(20)))? {
            assert!(Instant::now() < deadline, "the window never closed");
        }
        assert_eq!(asked.get(), 2);
        assert!(!window.is_alive());
        Ok(())
    }
}
