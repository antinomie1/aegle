//! Window documents, only in a dedicated test desktop: both paths apply the
//! window's theme before its content's transitions install, and a reload
//! validates host actions like the first build.
#![cfg(all(
    feature = "markup",
    feature = "motion",
    any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    )
))]

#[allow(unused_imports)]
use aegle::prelude::*;
use aegle::{App, AppOptions, Result, TextSystem, loader::Program};
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
