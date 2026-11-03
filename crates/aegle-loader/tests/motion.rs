//! Markup timings build the curves they name.
#![cfg(feature = "motion")]

use aegle_loader::{Elements, Program};
use aegle_ui::{Easing, Result, Spring, Transition, TransitionProperty};
use std::time::Duration;

mod common;

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
