//! Markup timings build the curves they name.
#![cfg(feature = "motion")]

use aegle_loader::{Elements, Program};
use aegle_ui::{Easing, Result, Spring, Transition, TransitionProperty};
use std::time::Duration;

mod common;

fn build(source: &'static str) -> Result<(aegle_ui::Ui, aegle_loader::View)> {
    let ui = common::ui()?;
    let program =
        Program::from_sources("main.aegle", &Elements::new(), &mut |_| Ok(source.into()))?;
    let view = program.build(&ui.root())?;
    Ok((ui, view))
}

#[test]
fn timings_name_presets_bezier_curves_and_springs() -> Result {
    let source = r#"Text { id: label
        paint_transition: [80ms, ease_in]
        scale_transition: [300ms, cubic_bezier(0.2, -0.5, 0.3, 1.4)]
        offset_transition: spring(170, 26)
        rotation_transition: [50ms, spring(300, 10)]
    }"#;
    let ui = common::ui()?;
    let program =
        Program::from_sources("main.aegle", &Elements::new(), &mut |_| Ok(source.into()))?;
    let view = program.build(&ui.root())?;
    let node = view.handle("label").unwrap().node();
    let timing = |property| node.property_transition(property).unwrap();
    let ms = Duration::from_millis;
    assert_eq!(
        timing(TransitionProperty::Paint),
        Transition::new(ms(80), Easing::EaseIn)
    );
    let bezier = Easing::cubic_bezier(0.2, -0.5, 0.3, 1.4).unwrap();
    assert_eq!(
        timing(TransitionProperty::Scale),
        Transition::new(ms(300), bezier)
    );
    let spring = Spring::new(170.0, 26.0).unwrap();
    assert_eq!(
        timing(TransitionProperty::Offset),
        Transition::spring(spring)
    );
    let bouncy = Easing::Spring(Spring::new(300.0, 10.0).unwrap());
    assert_eq!(
        timing(TransitionProperty::Rotation),
        Transition::new(ms(50), bouncy)
    );
    Ok(())
}

#[test]
fn animations_start_when_built_and_repeat_on_request() -> Result {
    let (ui, view) = build(
        r#"Column {
            Text { id: once; text: "a"; scale_animation: [[1, 1.5, 2], 100ms, ease_out] }
            Text { id: spin; text: "b"; rotation_animation: [[0, 360], 100ms, forever] }
            Text { id: pulse; text: "c"
                opacity_animation: [[0.4, 1], 100ms, ease_in_out, alternate] }
        }"#,
    )?;
    let node = |id| view.handle(id).unwrap().node().clone();
    let (once, spin, pulse) = (node("once"), node("spin"), node("pulse"));
    assert!(once.is_animating() && spin.is_animating() && pulse.is_animating());
    // The first refresh starts them; the second is past the once animation.
    for seconds in [1, 10] {
        ui.advance_animations(Duration::from_secs(seconds))?;
        ui.refresh()?;
    }
    assert!(!once.is_animating());
    assert_eq!(once.transform().scale, 2.0);
    assert!(spin.is_animating() && pulse.is_animating());
    spin.finish_transition();
    assert!((spin.transform().rotation - 360f32.to_radians()).abs() < 1e-4);
    Ok(())
}
