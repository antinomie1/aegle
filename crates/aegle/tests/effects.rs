//! Shadows, gradients, group opacity and backdrop blur written in markup reach
//! the same node state from `ui!` and the run-time engine, and their tokens
//! follow the theme.
#![cfg(all(feature = "markup", feature = "motion"))]

#[allow(unused_imports)]
use aegle::prelude::*;
use aegle::{
    loader::Elements,
    loader::Program,
    ui::{
        Color, Point, Result, Shadow, Size, TextSystem, Theme, TransitionProperty, Ui,
        register_token, scene::GradientGeometry,
    },
};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn raised(theme: &Theme) -> Shadow {
    Shadow {
        offset: Point::new(0.0, 2.0),
        blur: 6.0,
        spread: 0.0,
        color: Color(theme.foreground.0 & 0xffffff00 | 0x40),
    }
}

/// The effects of the fixture's card, caption and halo, after a theme switch.
fn check(ui: &Ui, card: &Node, caption: &Node, halo: &Node) -> Result {
    assert_eq!(card.shadow(), Some(raised(&Theme::light())));
    let gradient = card.background_gradient().unwrap();
    let GradientGeometry::Linear { start, end } = gradient.geometry() else {
        panic!("a linear gradient")
    };
    assert!((start.x - 0.0).abs() < 1e-6 && (start.y - 0.5).abs() < 1e-6);
    assert!((end.x - 1.0).abs() < 1e-6 && (end.y - 0.5).abs() < 1e-6);
    let colors = |node: &Node| -> Result<Vec<Color>> {
        let gradient = node.background_gradient().unwrap();
        Ok(gradient.stops().iter().map(|s| s.color).collect())
    };
    assert_eq!(colors(card)?, [Theme::light().accent, Color::WHITE]);
    assert_eq!(card.opacity(), 0.5);
    assert_eq!(card.backdrop_blur(), 6.0);
    let timing = card
        .property_transition(TransitionProperty::Opacity)
        .unwrap();
    assert_eq!(timing.duration, Duration::from_millis(120));
    let timing = card
        .property_transition(TransitionProperty::Shadow)
        .unwrap();
    assert_eq!(timing.duration, Duration::from_millis(200));
    assert_eq!(caption.shadow().unwrap().offset, Point::new(0.0, -1.0));
    let offsets: Vec<_> = (halo.background_gradient().unwrap().stops().iter())
        .map(|s| s.offset)
        .collect();
    assert_eq!(offsets, [0.0, 1.0]);
    // Both the stop and the shadow follow their tokens; the shadow tweens.
    ui.set_theme(Theme::dark());
    assert_eq!(colors(card)?, [Theme::dark().accent, Color::WHITE]);
    assert_eq!(card.shadow(), Some(raised(&Theme::dark())));
    assert!(ui.has_animations());
    Ok(())
}

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(320.0, 200.0));
    Ok(ui)
}

#[test]
fn effects_build_alike_from_both_paths() -> Result {
    register_token("studio.raised", raised)?;
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/effects.aegle")?;
    check(&compiled, &view.card, &view.caption, &view.halo)?;

    let loaded = ui()?;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/effects.aegle");
    let view = Program::load(path)?.build(&loaded.root())?;
    let node = |id| -> Result<Node> { Ok(view.handle(id).unwrap().node().clone()) };
    check(&loaded, &node("card")?, &node("caption")?, &node("halo")?)
}

#[test]
fn malformed_effects_are_rejected_before_building() {
    for (value, message) in [
        ("shadow: [0dp, 1dp, -2dp, 0dp, #000000]", "Shadow requires"),
        (
            "background_gradient: linear(90, #000000)",
            "BackgroundGradient requires",
        ),
        (
            "background_gradient: radial([#000000, 50%], #FFFFFF)",
            "BackgroundGradient requires",
        ),
        (
            "background_gradient: linear(0, [#000000, 60%], [#FFFFFF, 40%])",
            "BackgroundGradient requires",
        ),
        ("opacity: 1.5", "Opacity requires"),
        ("backdrop_blur: -1dp", "BackdropBlur requires"),
    ] {
        let source = format!("Column {{ {value} }}");
        let error =
            Program::from_sources("test.aegle", &Elements::new(), &mut |_| Ok(source.clone()))
                .unwrap_err()
                .to_string();
        assert!(error.contains(message), "{value}: {error}");
    }
}
