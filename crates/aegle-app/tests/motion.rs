//! Paint transitions share lifecycle, semantics and retained editing state.
#![cfg(feature = "motion")]
use aegle_app::{Color, Easing, ImeEdit, Result, Size, TextSystem, Theme, Transition, Ui};
use aegle_text::{Blob, GenericFamily, Selection, TextError};
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
    assert!(!ui.has_animations()); // Initial styling is not an entrance animation.
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
    assert!(!ui.has_animations());
    field.set_radius(30.0)?;
    ui.refresh()?;
    field.finish_transition()?;
    assert_eq!(field.presented_appearance()?.radius, 30.0);
    assert!(!ui.has_animations());
    field.set_radius(40.0)?;
    ui.refresh()?;
    ui.set_reduced_motion(true)?;
    assert!(!ui.has_animations());
    assert_eq!(field.presented_appearance()?.radius, 40.0);
    assert!(ui.refresh()?); // Reduced-motion setter must preserve the final repaint.
    field.set_radius(50.0)?;
    ui.refresh()?;
    assert_eq!(field.presented_appearance()?.radius, 50.0);
    assert!(!ui.has_animations());
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
    ui.visit_scenes(|scene, _, _| {
        fading_focus |= scene.commands().iter().any(|command| {
            matches!(command, aegle_scene::Command::Stroke { color, width, .. }
                if *color == Theme::light().accent && *width == 1.0)
        });
        Ok(())
    })?;
    assert!(fading_focus);
    field.cancel_transition()?;
    ui.refresh()?;
    assert!(!ui.has_animations());
    assert_eq!(field.presented_appearance()?.focus_width, 0.0);
    field.set_radius(60.0)?;
    ui.refresh()?;
    parent.set_visible(false)?;
    ui.refresh()?;
    assert!(!ui.has_animations());
    assert_eq!(field.presented_appearance()?.radius, 60.0);
    parent.set_visible(true)?;
    ui.refresh()?;
    field.set_transition(Transition::new(Duration::ZERO, Easing::Linear))?;
    field.set_radius(70.0)?;
    ui.refresh()?;
    assert!(!ui.has_animations());
    assert_eq!(field.presented_appearance()?.radius, 70.0);
    field.set_transition(timing)?;
    field.set_radius(80.0)?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(250))?;
    ui.refresh()?;
    assert!(!ui.has_animations());
    assert_eq!(field.presented_appearance()?.radius, 80.0);
    assert!(!ui.refresh()?);
    field.set_radius(90.0)?;
    ui.refresh()?;
    assert!(ui.has_animations());
    parent.remove()?;
    assert!(!ui.has_animations());
    assert!(field.finish_transition().is_err());
    ui.advance_animations(Duration::from_millis(300))?;
    ui.refresh()?;
    assert!(!ui.refresh()?);
    Ok(())
}
