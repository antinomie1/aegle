//! Window documents, only in a dedicated test desktop: both paths apply the
//! window's theme before its content's transitions install, and a reload
//! validates host actions like the first build.
#![cfg(all(feature = "markup", feature = "motion", feature = "native"))]

#[allow(unused_imports)]
use aegle::prelude::*;
use aegle::{
    app::{App, AppOptions},
    loader::Program,
    ui::{Result, TextSystem},
};
use std::time::Duration;

#[test]
#[ignore = "requires an isolated test desktop; never run on the user's desktop"]
fn window_documents_build_and_reload_like_fragments() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let app = App::with_fonts(TextSystem::new(), AppOptions::default())?;
    let compiled = aegle::ui!(&app, "tests/fixtures/window.aegle")?;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/window.aegle");
    let mut loaded = Program::load(path)?.open(&app)?;
    app.dispatch(Some(Duration::from_millis(50)))?;
    let panel = loaded.handle("panel").unwrap().node().clone();
    assert!(!compiled.panel.is_animating());
    assert!(!panel.is_animating());

    let source = "Window { Button { on clicked { host.missing() } } }";
    let missing =
        Program::from_sources("main.aegle", &aegle::loader::Elements::new(), &mut |_| {
            Ok(source.into())
        })?;
    let error = loaded.reload(&missing).unwrap_err().to_string();
    assert!(error.contains("`missing` is not registered"), "{error}");
    assert!(panel.is_alive());
    Ok(())
}

/// One markup file is one panel: it opens as a layer surface, a reload moves
/// it without recreating the surface or losing state, and the same document
/// opens once per output. `ui!` opens the static form alike.
#[cfg(feature = "wayland")]
#[test]
#[ignore = "requires an isolated layer-shell test desktop; never run on the user's desktop"]
fn layer_documents_open_as_panels_and_reload_in_place() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let app = App::with_options(AppOptions::default())?;
    let output = app.outputs()?.remove(0);
    let pump = |app: &App| -> Result {
        let deadline = std::time::Instant::now() + Duration::from_millis(300);
        while std::time::Instant::now() < deadline {
            app.dispatch(Some(Duration::from_millis(20)))?;
        }
        Ok(())
    };
    let compiled = aegle::ui!(&app, "tests/fixtures/layer.aegle")?;
    let read = |source: String| {
        Program::from_sources("panel.aegle", &aegle::loader::Elements::new(), &mut |_| {
            Ok(source.clone())
        })
    };
    let panel = |anchor: &str, zone: &str, output: &str| {
        format!(
            "Window {{ layer: top; anchor: {anchor}; exclusive_zone: {zone}; width: 300dp
                height: 28dp{output}
                state clicks: int = 0
                Text {{ id: label; text: \"clicks \" + str(clicks) }} }}"
        )
    };
    let named = format!("; output: \"{}\"", output.name);
    let mut view = read(panel("[top, left, right]", "28dp", &named))?.open(&app)?;
    let clicks = view.state::<i64>("clicks").unwrap();
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    let last = seen.clone();
    clicks.on_change(move |value| last.set(value));
    pump(&app)?;
    let window = view.root().typed::<aegle::app::Window>().unwrap();
    assert_eq!(window.bounds().size.width, output.size.width as f32);
    assert_eq!(compiled.root.bounds().size.width, output.size.width as f32);
    clicks.set(3)?;

    // Move to the bottom-left corner: same window, same state and handler.
    view.reload(&read(panel("[bottom, left]", "ignore", &named))?)?;
    pump(&app)?;
    let same = view.root().typed::<aegle::app::Window>().unwrap();
    assert_eq!(same.bounds().size.width, 300.0);
    assert!(window.is_alive());
    clicks.set(4)?;
    assert_eq!(seen.get(), 4);
    assert_eq!(view.get("clicks"), Some(aegle::loader::Data::Int(4)));

    // Turning the panel into a window or moving it to another output needs
    // a reopen; the panel survives the refused reload.
    let error = view
        .reload(&read("Window { Text {} }".into())?)
        .unwrap_err();
    assert!(
        error.to_string().contains("layer back into a window"),
        "{error}"
    );
    let elsewhere = panel("top", "0dp", "; output: \"nowhere\"");
    let error = view.reload(&read(elsewhere.clone())?).unwrap_err();
    assert!(error.to_string().contains("another output"), "{error}");
    assert!(read(elsewhere)?.open(&app).is_err());
    assert!(window.is_alive());

    // One document, one panel per output.
    let per_output = read(panel("top", "28dp", ""))?;
    let panels = app
        .outputs()?
        .into_iter()
        .map(|output| per_output.open_on(&app, output.id))
        .collect::<Result<Vec<_>>>()?;
    pump(&app)?;
    assert_eq!(panels.len(), app.outputs()?.len());
    Ok(())
}
