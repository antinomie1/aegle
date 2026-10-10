//! Group opacity and backdrop blur draw a subtree between a layer push and
//! pop, damage their whole subtree when they change, and a backdrop blur is
//! repainted whole once any damage reaches what it samples.

use std::{cell::RefCell, rc::Rc};

use aegle_ui::{Color, Rect, Result, Size, TextSystem, Theme, Ui, Visit, scene::Layer};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Push(f32, f32),
    Scene,
    Pop,
}

fn steps(ui: &Ui) -> Result<(Vec<Step>, Vec<Layer>)> {
    ui.refresh()?;
    let (mut steps, mut layers) = (Vec::new(), Vec::new());
    ui.visit_scenes(|visit| {
        steps.push(match visit {
            Visit::Scene { .. } => Step::Scene,
            Visit::PushLayer(layer) => {
                layers.push(layer);
                Step::Push(layer.opacity(), layer.backdrop_blur())
            }
            Visit::PopLayer => Step::Pop,
        });
        Ok(())
    })?;
    Ok((steps, layers))
}

fn damage(ui: &Ui) -> Result<Vec<Rect>> {
    Ok(ui.damage().expect("partial damage").rects().to_vec())
}

#[test]
fn groups_draw_through_layers_and_damage_their_subtree() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(20.0);
    ui.root().set_background(Color::WHITE);
    ui.resize(Size::new(300.0, 100.0));
    let panel = ui.root().column();
    panel.set_padding(0.0);
    panel.set_gap(0.0, 0.0);
    panel.set_width(80.0);
    panel.set_height(40.0);
    panel.set_background(Color::BLACK);
    let child = panel.column();
    child.set_width(20.0);
    child.set_height(20.0);
    child.set_background(Color::rgb(200, 0, 0));
    let near = panel.column();
    near.set_width(20.0);
    near.set_height(20.0);
    near.set_background(Color::BLACK);
    ui.refresh()?;
    ui.clear_damage();

    assert!(panics(|| {
        panel.set_opacity(1.5);
    }));
    assert!(panics(|| {
        panel.set_backdrop_blur(-1.0);
    }));
    panel.set_opacity(0.5);
    let (found, layers) = steps(&ui)?;
    use Step::*;
    assert_eq!(found, [Scene, Push(0.5, 0.0), Scene, Scene, Scene, Pop]);
    assert_eq!(layers[0].extent(), panel.bounds());
    // The group's whole subtree changes, not only the panel's own record.
    assert_eq!(damage(&ui)?, [panel.bounds()]);
    ui.clear_damage();

    panel.set_opacity(0.0);
    let (found, _) = steps(&ui)?;
    assert_eq!(found, [Scene], "a transparent subtree is skipped");
    panel.set_opacity(1.0);
    let (found, _) = steps(&ui)?;
    assert_eq!(found, [Scene; 4], "opaque needs no layer");
    ui.clear_damage();

    // A change beside a blurred node, within its reach, repaints the whole
    // sampled area: 3σ + 2 around the node.
    child.set_backdrop_blur(4.0);
    let (found, _) = steps(&ui)?;
    assert_eq!(found, [Scene, Scene, Push(1.0, 4.0), Scene, Pop, Scene]);
    ui.clear_damage();
    near.set_background(Color::rgb(0, 0, 200));
    ui.refresh()?;
    let c = child.bounds();
    let sampled = Rect::new(
        c.origin.x - 14.0,
        c.origin.y - 14.0,
        c.size.width + 28.0,
        c.size.height + 28.0,
    );
    assert!(sampled.intersection(near.bounds()).is_some());
    let rects = damage(&ui)?;
    let covers = |r: &Rect| {
        r.origin.x <= sampled.origin.x
            && r.origin.y <= sampled.origin.y
            && r.origin.x + r.size.width >= sampled.origin.x + sampled.size.width
            && r.origin.y + r.size.height >= sampled.origin.y + sampled.size.height
    };
    assert!(rects.iter().any(covers), "{rects:?}");
    Ok(())
}

#[cfg(feature = "motion")]
#[test]
fn opacity_follows_its_transition() -> Result {
    use aegle_ui::{Easing, Transition, TransitionProperty};
    use std::time::Duration;

    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0));
    let panel = ui.root().column();
    panel.set_width(40.0);
    panel.set_height(20.0);
    panel.set_background(Color::BLACK);
    ui.refresh()?;
    let linear = Transition::new(Duration::from_millis(100), Easing::Linear);
    panel.set_property_transition(TransitionProperty::Opacity, Some(linear));
    panel.set_opacity(0.0);
    assert_eq!(panel.opacity(), 0.0, "reads the target");
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(25))?;
    let (_, layers) = steps(&ui)?;
    assert!((layers[0].opacity() - 0.75).abs() < 1e-4);
    ui.advance_animations(Duration::from_millis(100))?;
    let (found, _) = steps(&ui)?;
    assert!(!found.contains(&Step::Pop) && !ui.has_animations());
    Ok(())
}

/// Whether `change` panics, as handle methods do on rejected values.
fn panics(change: impl FnOnce()) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(change)).is_err()
}
