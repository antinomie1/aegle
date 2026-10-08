//! Offset, scale and rotation follow their own transition timings and the
//! control completes once, after the last of them.
#![cfg(feature = "motion")]

use std::{cell::Cell, cell::RefCell, rc::Rc, time::Duration};

use aegle_ui::{
    Animate, Animation, Color, Cycles, Easing, Keyframe, Point, Result, Size, TextSystem, Theme,
    Transform, Transition, TransitionProperty as Property, Ui,
};

/// The presented scale and rotation of the first visible scene.
fn presented(ui: &Ui) -> Result<(f32, f32)> {
    let mut spin = None;
    ui.visit_scenes(|visit| {
        let aegle_ui::Visit::Scene { transform, .. } = visit else {
            return Ok(());
        };
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

#[test]
fn scoped_timing_and_explicit_animations_override_the_policy() -> Result {
    let ms = Duration::from_millis;
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    ui.resize(Size::new(300.0, 100.0))?;
    let panel = ui.root().column()?;
    panel.set_size(40.0, 20.0)?;
    panel.set_background(Color::BLACK)?;
    ui.refresh()?;
    let x = || panel.bounds().map(|b| b.origin.x);
    let step = |at| -> Result {
        ui.advance_animations(ms(at))?;
        ui.refresh().map(drop)
    };

    // Without a policy, a with_transition closure animates paint and offset.
    let linear = Transition::new(ms(100), Easing::Linear);
    panel.with_transition(linear, || {
        panel.set_background(Color::WHITE)?;
        panel.set_offset(Point::new(100.0, 0.0))
    })?;
    step(0)?;
    step(50)?;
    assert_eq!(x()?, 50.0);
    let [r, ..] = panel.presented_appearance()?.background.to_rgba();
    assert!(0 < r && r < 255 && panel.is_animating()?);
    step(100)?;
    // Later changes snap again: the closure's timing ended with it.
    panel.set_offset(Point::new(0.0, 0.0))?;
    ui.refresh()?;
    assert!(x()? == 0.0 && !panel.is_animating()?);

    // snap skips the policy for its changes only.
    panel.set_transition(linear)?;
    panel.snap(|| {
        panel.set_background(Color::BLACK)?;
        panel.set_offset(Point::new(30.0, 0.0))
    })?;
    step(100)?;
    assert_eq!(x()?, 30.0);
    assert_eq!(panel.presented_appearance()?, panel.appearance()?);
    assert!(!panel.is_animating()?);

    // An explicit keyframe animation completes like a transition.
    let ends = Rc::new(Cell::new(0));
    let count = ends.clone();
    panel.on_transition_end(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    let frames = [
        Keyframe::new(0.0, Point::new(0.0, 0.0)),
        Keyframe::new(0.5, Point::new(80.0, 0.0)),
        Keyframe::new(1.0, Point::new(10.0, 0.0)),
    ];
    panel.animate(Animate::Offset(Animation::new(ms(100), frames)?))?;
    assert_eq!(
        panel.offset()?,
        Point::new(10.0, 0.0),
        "the target is the last frame"
    );
    step(100)?;
    step(150)?;
    assert_eq!(x()?, 80.0);
    step(200)?;
    ui.dispatch_callbacks()?;
    assert!(x()? == 10.0 && !panel.is_animating()? && ends.get() == 1);

    // A forever animation runs until stopped.
    let spin = Animation::tween(0.0, 1.0, linear)?.cycles(Cycles::Forever);
    panel.animate(Animate::Rotation(spin))?;
    step(200)?;
    step(100_000)?;
    assert!(panel.is_animating()? && ui.has_animations()?);
    panel.finish_transition()?;
    assert!(!panel.is_animating()? && panel.transform()?.rotation == 1.0);

    // Reduced motion goes straight to the target.
    ui.set_reduced_motion(true)?;
    let grow = Animation::tween(0.5, 2.0, linear)?;
    panel.animate(Animate::Scale(grow))?;
    assert!(!panel.is_animating()? && panel.transform()?.scale == 2.0);
    Ok(())
}
