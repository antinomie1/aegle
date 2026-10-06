//! Markup geometry and per-property transitions build the same timings from
//! compiled and runtime-loaded markup, and bound geometry eases on change.
#![cfg(all(feature = "markup", feature = "motion"))]

use aegle::{
    Easing, Node, Point, Result, Size, TextSystem, Theme, Transition, TransitionProperty, Ui,
    loader::{Data, Program},
};
use std::{cell::RefCell, f32::consts::FRAC_PI_2, rc::Rc, time::Duration};

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0))?;
    Ok(ui)
}

fn check(ui: &Ui, panel: &Node, zoom: &dyn Fn(f32) -> Result) -> Result {
    let timing = |ms, easing| Some(Transition::new(Duration::from_millis(ms), easing));
    let timings = [
        (TransitionProperty::Paint, timing(50, Easing::EaseOut)),
        (TransitionProperty::Offset, timing(200, Easing::Linear)),
        (TransitionProperty::Scale, timing(100, Easing::EaseOut)),
        (TransitionProperty::Rotation, timing(50, Easing::EaseOut)),
    ];
    for (property, timing) in timings {
        assert_eq!(panel.property_transition(property)?, timing, "{property:?}");
    }
    ui.refresh()?;
    assert_eq!(panel.offset()?, Point::new(10.0, 4.0));
    let transform = panel.transform()?;
    assert_eq!(transform.scale, 1.0);
    assert!((transform.rotation - FRAC_PI_2).abs() < 1e-6);
    zoom(2.0)?;
    ui.refresh()?;
    assert_eq!(panel.transform()?.scale, 2.0);
    assert!(panel.is_animating()?);
    ui.advance_animations(Duration::from_millis(100))?;
    assert!(!panel.is_animating()?);
    Ok(())
}

#[test]
fn compiled_and_loaded_geometry_match() -> Result {
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/motion.aegle")?;
    let state = view.zoom.clone();
    check(&compiled, &view.panel, &|zoom| state.set(zoom))?;

    let loaded = ui()?;
    let program = Program::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/motion.aegle"
    ))?;
    let runtime = program.build(&loaded.root())?;
    let panel = runtime.handle("panel").unwrap().node().clone();
    check(&loaded, &panel, &|zoom| {
        runtime.set("zoom", Data::Float(zoom))
    })
}
