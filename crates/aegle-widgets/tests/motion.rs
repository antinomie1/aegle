//! Paint transitions share lifecycle, semantics and retained editing state.
#![cfg(feature = "motion")]
use aegle_text::{Blob, GenericFamily, Selection, TextError};
use aegle_ui::{Color, Easing, ImeEdit, Result, Size, TextSystem, Theme, Transition, Ui};
use aegle_widgets::*;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

#[test]
fn transitions_retarget_and_stop_without_disturbing_editing() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let timing = Transition::new(Duration::from_millis(100), Easing::Linear);
    ui.set_default_transition(Some(timing))?;
    let parent = ui.root().column()?;
    let field = parent.text_field("Hello")?;
    field.set_foreground(Color::BLACK)?;
    field.set_radius(0.0)?;
    field.focus()?;
    ui.resize(Size::new(320.0, 160.0))?;
    ui.refresh()?;
    assert!(!ui.has_animations()?); // Initial styling is not an entrance animation.
    ui.take_ime_state(4000)?;
    ui.ime(ImeEdit {
        preedit: "世界",
        ..Default::default()
    })?;
    field.set_foreground(Color::WHITE)?;
    field.set_radius(10.0)?;
    ui.refresh()?;
    assert!(field.is_animating()?);
    assert_eq!(field.presented_appearance()?.radius, 0.0);
    ui.advance_animations(Duration::from_millis(50))?;
    ui.refresh()?;
    let midpoint = field.presented_appearance()?;
    assert_eq!(midpoint.radius, 5.0);
    assert_eq!(midpoint.foreground, Color::rgb(188, 188, 188));
    #[cfg(feature = "accessibility")]
    assert!(
        ui.accessibility(true, "Motion")?
            .nodes
            .iter()
            .any(|(_, node)| {
                node.foreground_color().is_some_and(|color| {
                    [color.red, color.green, color.blue, color.alpha]
                        == midpoint.foreground.to_rgba()
                })
            })
    );
    field.set_radius(20.0)?;
    ui.refresh()?;
    assert_eq!(field.presented_appearance()?, midpoint);
    ui.advance_animations(Duration::from_millis(100))?;
    ui.refresh()?;
    assert_eq!(field.presented_appearance()?.radius, 12.5);
    assert!(ui.advance_animations(Duration::from_millis(99)).is_err());
    field.cancel_transition()?;
    ui.refresh()?;
    assert_eq!(field.appearance()?, field.presented_appearance()?);
    assert!(!ui.has_animations()?);
    field.set_radius(30.0)?;
    ui.refresh()?;
    field.finish_transition()?;
    assert_eq!(field.presented_appearance()?.radius, 30.0);
    assert!(!ui.has_animations()?);
    field.set_radius(40.0)?;
    ui.refresh()?;
    ui.set_reduced_motion(true)?;
    assert!(!ui.has_animations()?);
    assert_eq!(field.presented_appearance()?.radius, 40.0);
    assert!(ui.refresh()?); // Reduced-motion setter must preserve the final repaint.
    field.set_radius(50.0)?;
    ui.refresh()?;
    assert_eq!(field.presented_appearance()?.radius, 50.0);
    assert!(!ui.has_animations()?);
    assert_eq!(field.text()?, "Hello");
    let error = field.select(Selection::default()).unwrap_err();
    assert_eq!(error.downcast_ref(), Some(&TextError::CompositionActive));
    let ime = ui.take_ime_state(4000)?;
    assert!(ime.is_none_or(|state| !state.reset));
    ui.ime(ImeEdit {
        commit: Some("你好"),
        ..Default::default()
    })?;
    assert!(field.text()?.contains("你好"));
    ui.set_reduced_motion(false)?;
    ui.window_focus(false)?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(150))?;
    ui.refresh()?;
    assert_eq!(field.appearance()?.focus_width, 0.0);
    assert_eq!(field.presented_appearance()?.focus_width, 1.0);
    let mut fading_focus = false;
    ui.visit_scenes(|visit| {
        let aegle_ui::Visit::Scene { scene, .. } = visit else {
            return Ok(());
        };
        fading_focus |= scene.commands().iter().any(|command| {
            matches!(command, aegle_scene::Command::Stroke { color, width, .. }
                if *color == Theme::light().accent && *width == 1.0)
        });
        Ok(())
    })?;
    assert!(fading_focus);
    field.cancel_transition()?;
    ui.refresh()?;
    assert!(!ui.has_animations()?);
    assert_eq!(field.presented_appearance()?.focus_width, 0.0);
    field.set_radius(60.0)?;
    ui.refresh()?;
    parent.set_visible(false)?;
    ui.refresh()?;
    assert!(!ui.has_animations()?);
    assert_eq!(field.presented_appearance()?.radius, 60.0);
    parent.set_visible(true)?;
    ui.refresh()?;
    field.set_transition(Transition::new(Duration::ZERO, Easing::Linear))?;
    field.set_radius(70.0)?;
    ui.refresh()?;
    assert!(!ui.has_animations()?);
    assert_eq!(field.presented_appearance()?.radius, 70.0);
    field.set_transition(timing)?;
    field.set_radius(80.0)?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(250))?;
    ui.refresh()?;
    assert!(!ui.has_animations()?);
    assert_eq!(field.presented_appearance()?.radius, 80.0);
    assert!(!ui.refresh()?);
    field.set_radius(90.0)?;
    ui.refresh()?;
    assert!(ui.has_animations()?);
    parent.remove()?;
    assert!(!ui.has_animations()?);
    assert!(field.finish_transition().is_err());
    ui.advance_animations(Duration::from_millis(300))?;
    ui.refresh()?;
    assert!(!ui.refresh()?);
    Ok(())
}

#[test]
fn offsets_move_hit_testing_and_complete_once() -> Result {
    use aegle_ui::{Modifiers, Point, PointerId, PointerKind};
    use std::cell::Cell;
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let panel = ui.root().column()?;
    let button = panel.button("")?;
    button.set_size(Some(40.0), Some(20.0))?;
    ui.resize(Size::new(300.0, 100.0))?;
    panel.set_offset(Point::new(50.0, 0.0))?; // No policy: applies at once.
    ui.refresh()?;
    assert_eq!(button.bounds()?.origin, Point::new(50.0, 0.0));
    let ends = Rc::new(Cell::new(0));
    let count = ends.clone();
    panel.on_transition_end(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    panel.set_transition(Transition::new(Duration::from_millis(100), Easing::Linear))?;
    panel.set_offset(Point::new(150.0, 0.0))?;
    ui.refresh()?; // Starts at the current host time.
    ui.advance_animations(Duration::from_millis(50))?;
    ui.refresh()?;
    assert_eq!(panel.offset()?.x, 150.0);
    assert_eq!(button.bounds()?.origin.x, 100.0);
    // Hit testing follows the presented offset.
    ui.pointer(
        PointerId(1),
        PointerKind::Move,
        Point::new(110.0, 10.0),
        Modifiers::default(),
    )?;
    assert!(button.visual_state()?.hovered);
    ui.advance_animations(Duration::from_millis(100))?;
    ui.refresh()?;
    ui.dispatch_callbacks()?;
    assert_eq!((button.bounds()?.origin.x, ends.get()), (150.0, 1));
    assert!(!ui.has_animations()?);
    // Cancel freezes the presented offset without completing.
    panel.set_offset(Point::new(50.0, 0.0))?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(150))?;
    panel.cancel_transition()?;
    ui.refresh()?;
    assert_eq!(panel.offset()?.x, 100.0);
    panel.set_offset(Point::new(0.0, 0.0))?;
    panel.finish_transition()?;
    ui.set_reduced_motion(true)?;
    panel.set_offset(Point::new(10.0, 0.0))?;
    ui.dispatch_callbacks()?;
    assert_eq!((panel.offset()?.x, ends.get()), (10.0, 3));
    ui.set_reduced_motion(false)?;
    panel.set_offset(Point::new(20.0, 0.0))?;
    assert!(panel.is_animating()?);
    panel.remove()?;
    ui.dispatch_callbacks()?;
    assert_eq!(ends.get(), 3);
    assert!(!ui.has_animations()?);
    assert!(button.set_offset(Point::new(f32::NAN, 0.0)).is_err());
    Ok(())
}

#[test]
fn scale_and_rotation_move_scenes_and_hit_testing() -> Result {
    use aegle_ui::{Modifiers, Point, PointerId, PointerKind, Transform};
    use std::cell::Cell;
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let button = ui.root().button("")?;
    button.set_size(Some(40.0), Some(20.0))?;
    ui.resize(Size::new(300.0, 100.0))?;
    ui.refresh()?;
    let ends = Rc::new(Cell::new(0));
    let count = ends.clone();
    button.on_transition_end(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    button.set_transition(Transition::new(Duration::from_millis(100), Easing::Linear))?;
    button.set_transform(Transform {
        scale: 2.0,
        rotation: 0.0,
    })?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(50))?;
    ui.refresh()?;
    assert_eq!(button.transform()?.scale, 2.0, "target, not presented");
    let hover = |x| -> Result<bool> {
        ui.pointer(
            PointerId(1),
            PointerKind::Move,
            Point::new(x, 10.0),
            Modifiers::default(),
        )?;
        button.visual_state().map(|s| s.hovered)
    };
    // Halfway: scale 1.5 about the center (20, 10) covers x in -10..50.
    assert!(hover(48.0)?);
    assert!(!hover(52.0)?);
    ui.advance_animations(Duration::from_millis(100))?;
    ui.refresh()?;
    ui.dispatch_callbacks()?;
    assert_eq!(ends.get(), 1);
    assert!(hover(58.0)?);
    let mut origin = None;
    ui.visit_scenes(|visit| {
        let aegle_ui::Visit::Scene { transform, .. } = visit else {
            return Ok(());
        };
        let [a, _, _, d, e, _] = transform.coefficients();
        origin.get_or_insert((a, d, e));
        Ok(())
    })?;
    let (a, d, e) = origin.expect("a visible scene");
    assert_eq!((a, d), (2.0, 2.0));
    assert!(
        e <= 0.0,
        "scaled about the center, so it starts left of zero"
    );
    button.set_transform(Transform {
        scale: 1.0,
        rotation: std::f32::consts::PI,
    })?;
    button.finish_transition()?;
    ui.refresh()?;
    assert!(
        ui.has_animations()? || hover(38.0)?,
        "a half turn keeps the center"
    );
    assert!(
        button
            .set_transform(Transform {
                scale: 0.0,
                rotation: 0.0
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn flings_decay_then_stop_at_edges_input_or_reduced_motion() -> Result {
    use aegle_ui::Point;
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let view = ui.root().scroll_view()?;
    view.set_size(Some(100.0), Some(100.0))?;
    view.set_padding(0.0)?;
    view.set_gap(0.0)?;
    for _ in 0..10 {
        view.button("")?.set_height(Some(40.0))?;
    }
    ui.resize(Size::new(200.0, 200.0))?;
    ui.refresh()?;
    let at = Point::new(10.0, 10.0);
    let step = |ms| -> Result {
        ui.advance_animations(Duration::from_millis(ms))?;
        ui.refresh()?;
        Ok(())
    };
    ui.fling(at, Point::new(0.0, 600.0))?;
    assert!(ui.has_animations()?);
    step(1000)?; // The request starts at the host's current time.
    assert_eq!(view.offset()?.y, 0.0);
    step(1100)?;
    let first = view.offset()?.y;
    assert!(
        (45.0..58.0).contains(&first),
        "600 * 0.325 * (1 - e^-0.31): {first}"
    );
    step(11_000)?;
    let total = view.offset()?.y;
    assert!(
        (190.0..196.0).contains(&total),
        "travels velocity * 0.325: {total}"
    );
    assert!(!ui.has_animations()?);
    // The first edge ends the fling, whatever speed remains.
    view.scroll_to(Point::new(0.0, 0.0))?;
    ui.fling(at, Point::new(0.0, -600.0))?;
    step(12_000)?;
    step(12_100)?;
    assert!(!ui.has_animations()?);
    // Any new scroll or press cancels; reduced motion and slow flings never start.
    ui.fling(at, Point::new(0.0, 600.0))?;
    ui.scroll_by(at, Point::new(0.0, 1.0))?;
    assert!(!ui.has_animations()?);
    ui.fling(at, Point::new(0.0, 600.0))?;
    ui.stop_fling()?;
    ui.fling(at, Point::new(0.0, 5.0))?;
    assert!(!ui.has_animations()?);
    ui.set_reduced_motion(true)?;
    ui.fling(at, Point::new(0.0, 600.0))?;
    assert!(!ui.has_animations()?);
    assert!(ui.fling(at, Point::new(f32::NAN, 0.0)).is_err());
    Ok(())
}
