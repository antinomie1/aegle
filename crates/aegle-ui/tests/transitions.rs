//! Offset, scale and rotation follow their own transition timings and the
//! control completes once, after the last of them.
#![cfg(feature = "motion")]

use std::{cell::Cell, cell::RefCell, rc::Rc, time::Duration};

use aegle_ui::{
    Color, Easing, Point, Result, Size, TextSystem, Theme, Transform, Transition,
    TransitionProperty as Property, Ui,
};

/// The presented scale and rotation of the first visible scene.
fn presented(ui: &Ui) -> Result<(f32, f32)> {
    let mut spin = None;
    ui.visit_scenes(|_, transform, _| {
        let [a, b, ..] = transform.coefficients();
        spin.get_or_insert(((a * a + b * b).sqrt(), b.atan2(a)));
        Ok(())
    })?;
    Ok(spin.expect("a visible scene"))
}

#[test]
fn geometric_properties_follow_their_own_timing() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    ui.resize(Size::new(300.0, 100.0))?;
    let panel = ui.root().column()?;
    panel.set_size(40.0, 20.0)?;
    panel.set_background(Color::rgb(0, 0, 0))?;
    ui.refresh()?;
    let ends = Rc::new(Cell::new(0));
    let count = ends.clone();
    panel.on_transition_end(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    let linear = |ms| Some(Transition::new(Duration::from_millis(ms), Easing::Linear));
    panel.set_property_transition(Property::Offset, linear(200))?;
    panel.set_property_transition(Property::Scale, linear(100))?;
    assert_eq!(panel.property_transition(Property::Rotation)?, None);
    panel.set_offset(Point::new(100.0, 0.0))?;
    panel.set_transform(Transform {
        scale: 2.0,
        rotation: 1.0,
    })?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(50))?;
    ui.refresh()?;
    // Rotation has no timing and jumps; offset and scale ease separately.
    let (scale, rotation) = presented(&ui)?;
    assert!((scale - 1.5).abs() < 1e-4 && (rotation - 1.0).abs() < 1e-4);
    assert_eq!(panel.bounds()?.origin.x, 25.0);
    ui.advance_animations(Duration::from_millis(100))?;
    ui.refresh()?;
    ui.dispatch_callbacks()?;
    assert!((presented(&ui)?.0 - 2.0).abs() < 1e-4);
    assert_eq!((panel.bounds()?.origin.x, ends.get()), (50.0, 0));
    ui.advance_animations(Duration::from_millis(200))?;
    ui.refresh()?;
    ui.dispatch_callbacks()?;
    assert_eq!((panel.bounds()?.origin.x, ends.get()), (100.0, 1));
    assert!(!ui.has_animations()?);

    // Removing one timing jumps that property to its target, silently.
    panel.set_offset(Point::new(0.0, 0.0))?;
    ui.refresh()?;
    assert!(panel.is_animating()?);
    panel.set_property_transition(Property::Offset, None)?;
    ui.refresh()?;
    ui.dispatch_callbacks()?;
    assert_eq!((panel.bounds()?.origin.x, ends.get()), (0.0, 1));
    assert!(!panel.is_animating()?);
    panel.set_property_transition(Property::Scale, None)?;
    assert_eq!(panel.property_transition(Property::Scale)?, None);
    Ok(())
}
