//! Window management through the host on a real macOS window server: the
//! options a window opens with, its mode and size methods, a close request
//! the window's handler declines once, and an undecorated window whose
//! title bar is a control. Opt-in on a GUI session:
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
    macos::title_bar()?;
    println!("window_management: an undecorated window with a title bar ... ok");
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
    use aegle_ui::{InputKind, Point, PointerKind, Result, TextSystem, WindowArea};
    use aegle_widgets::Widgets;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{MainThreadMarker, NSPoint, NSString};
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

    /// Queues a primary press and release for the AppKit window titled
    /// `title` at `at`, in the logical coordinates of its content.
    fn click(title: &str, at: Point) {
        let mtm = MainThreadMarker::new().expect("the test runs on the main thread");
        let app = NSApplication::sharedApplication(mtm);
        let title = NSString::from_str(title);
        let windows = app.windows();
        let window = windows
            .iter()
            .find(|window| window.title() == title)
            .expect("the window is on screen");
        // Window coordinates grow upward from the bottom-left corner.
        let height = window.contentRectForFrameRect(window.frame()).size.height;
        let location = NSPoint::new(f64::from(at.x), height - f64::from(at.y));
        for kind in [NSEventType::LeftMouseDown, NSEventType::LeftMouseUp] {
            let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                kind,
                location,
                NSEventModifierFlags::empty(),
                0.0,
                window.windowNumber(),
                None,
                0,
                1,
                1.0,
            )
            .expect("a mouse event");
            app.postEvent_atStart(&event, false);
        }
    }

    /// An undecorated window whose title bar is a row marked as a move
    /// area: presses on the bar go to the window system and never reach the
    /// controls, while the button inside the bar still clicks.
    pub fn title_bar() -> Result<()> {
        let app = App::with_fonts(fonts()?, AppOptions::default())?;
        let title = "Aegle host title bar";
        let window = app.window_with_options(
            title,
            WindowOptions {
                width: 360,
                height: 240,
                decorations: false,
                ..Default::default()
            },
        )?;
        let bar = window.row();
        bar.set_window_area(WindowArea::Move);
        let label = bar.text("标题");
        let button = bar.button("Close");
        window.text("你好");
        let (presses, clicks) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let count = presses.clone();
        window.on_input_capture(move |_, event| {
            if let InputKind::Pointer {
                kind: PointerKind::Down { .. },
                ..
            } = event.kind
            {
                count.set(count.get() + 1);
            }
        });
        let count = clicks.clone();
        button.on_click(move |_| count.set(count.get() + 1));
        let pump = |millis| -> Result<()> {
            let deadline = Instant::now() + Duration::from_millis(millis);
            while Instant::now() < deadline {
                app.dispatch(Some(Duration::from_millis(10)))?;
            }
            Ok(())
        };
        pump(300)?;
        // The content kept its size without the title bar.
        assert_eq!(window.size()?, (360, 240));
        let center = |bounds: aegle_ui::Rect| {
            Point::new(
                bounds.origin.x + bounds.size.width / 2.0,
                bounds.origin.y + bounds.size.height / 2.0,
            )
        };
        // On the label, inside the bar: the window system takes the press.
        click(title, center(label.bounds()));
        pump(200)?;
        assert_eq!((presses.get(), clicks.get()), (0, 0));
        // On the button, which lies over the bar: an ordinary click.
        click(title, center(button.bounds()));
        pump(200)?;
        assert_eq!((presses.get(), clicks.get()), (1, 1));
        // Unmarked, the bar is content again.
        bar.set_window_area(None);
        pump(100)?;
        click(title, center(label.bounds()));
        pump(200)?;
        assert_eq!((presses.get(), clicks.get()), (2, 1));

        window.set_decorations(true)?;
        assert_eq!(window.size()?, (360, 240));
        window.set_decorations(false)?;
        assert_eq!(window.size()?, (360, 240));
        // macOS windows take their icon from the application.
        let icon = aegle_app::Icon {
            width: 1,
            height: 1,
            rgba: &[255, 0, 0, 255],
        };
        let error = window.set_icon(icon).expect_err("windows have no icons");
        assert!(error.to_string().contains("unsupported"), "{error}");
        window.close()?;
        Ok(())
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
