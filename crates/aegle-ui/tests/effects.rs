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
    panel.set_size(80.0, 40.0)?;
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
