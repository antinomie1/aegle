//! A node's shadow is recorded beneath its gradient background, which is
//! sized to the node and replaces its background color.

use std::{cell::RefCell, rc::Rc};

use aegle_ui::{
    Color, Point, Result, Shadow, Size, TextSystem, Theme, Ui,
    scene::{Command, Gradient, GradientGeometry, GradientStop},
};

#[test]
fn shadows_and_gradient_backgrounds() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0))?;
    let panel = ui.root().column()?;
    panel.set_width(80.0)?;
    panel.set_height(40.0)?;
    panel.set_radius(6.0)?;
    panel.set_background(Color::rgb(0, 0, 0))?;
    let stop = |offset, color| GradientStop { offset, color };
    let stops = [stop(0.0, Color::WHITE), stop(1.0, Color::BLACK)];
    let gradient = Gradient::linear(Point::new(0.0, 0.0), Point::new(0.0, 1.0), &stops)?;
    let shadow = Shadow {
        offset: Point::new(0.0, 4.0),
        blur: 6.0,
        spread: 2.0,
        color: Color::rgba(0, 0, 0, 80),
    };
    let invalid = Shadow {
        blur: -1.0,
        ..shadow
    };
    assert!(panel.set_shadow(Some(invalid)).is_err());
    panel.set_shadow(Some(shadow))?;
    panel.set_background_gradient(Some(gradient.clone()))?;
    assert_eq!(
        (panel.shadow()?, panel.background_gradient()?),
        (Some(shadow), Some(gradient))
    );
    let commands = |ui: &Ui| -> Result<Vec<Command>> {
        ui.refresh()?;
        let mut found = Vec::new();
        ui.visit_scenes(|visit| {
            let aegle_ui::Visit::Scene { scene, .. } = visit else {
                return Ok(());
            };
            if let [Command::Shadow { .. }, ..] = scene.commands() {
                found = scene.commands().to_vec();
                let gradients = scene.gradients();
                let end = Point::new(0.0, 40.0);
                assert!(gradients.iter().all(|g| matches!(
                    g.geometry(),
                    GradientGeometry::Linear { end: e, .. } if e == end
                )));
            }
            Ok(())
        })?;
        Ok(found)
    };
    let found = commands(&ui)?;
    let [
        Command::Shadow { shape, blur, .. },
        Command::FillGradient { .. },
        ..,
    ] = found[..]
    else {
        panic!("shadow and gradient, got {found:?}");
    };
    assert_eq!(
        (shape.rect().origin, shape.radius(), blur),
        (Point::new(-2.0, 2.0), 8.0, 6.0)
    );

    panel.set_shadow(None)?;
    panel.set_background_gradient(None)?;
    assert!(commands(&ui)?.is_empty());
    Ok(())
}

/// MD3 elevation: a shadow fades in, retargets mid-flight, and fades out
/// until it is gone, asking for frames only while it moves.
#[cfg(feature = "motion")]
#[test]
fn shadows_follow_their_transition() -> Result {
    use aegle_ui::{Easing, Transition, TransitionProperty};
    use std::time::Duration;

    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0))?;
    let card = ui.root().column()?;
    card.set_width(40.0)?;
    card.set_height(20.0)?;
    ui.refresh()?;
    let linear = Transition::new(Duration::from_millis(100), Easing::Linear);
    card.set_property_transition(TransitionProperty::Shadow, Some(linear))?;
    let raised = Shadow {
        offset: Point::new(0.0, 4.0),
        blur: 8.0,
        spread: 0.0,
        color: Color::rgba(0, 0, 0, 200),
    };
    card.set_shadow(Some(raised))?;
    assert_eq!(card.shadow()?, Some(raised), "reads the target");
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(50))?;
    let shadow = |ui: &Ui| -> Result<Option<(f32, u8)>> {
        ui.refresh()?;
        let mut found = None;
        ui.visit_scenes(|visit| {
            if let aegle_ui::Visit::Scene { scene, .. } = visit {
                for command in scene.commands() {
                    if let Command::Shadow { blur, color, .. } = command {
                        found = Some((*blur, color.to_rgba()[3]));
                    }
                }
            }
            Ok(())
        })?;
        Ok(found)
    };
    // Halfway: same geometry, half the alpha.
    assert_eq!(shadow(&ui)?, Some((8.0, 100)));
    // Retargeting starts from the shown shadow.
    card.set_shadow(Some(Shadow {
        blur: 16.0,
        ..raised
    }))?;
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(100))?;
    let (blur, _) = shadow(&ui)?.unwrap();
    assert!((blur - 12.0).abs() < 1e-3, "{blur}");
    ui.advance_animations(Duration::from_millis(200))?;
    assert_eq!(shadow(&ui)?, Some((16.0, 200)));
    assert!(!ui.has_animations()? && !ui.wants_frames()?);
    // Fading out removes it.
    card.set_shadow(None)?;
    assert_eq!(card.shadow()?, None);
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(400))?;
    assert_eq!(shadow(&ui)?, None);
    assert!(!ui.has_animations()?);
    Ok(())
}
