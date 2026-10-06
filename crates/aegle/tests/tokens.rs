//! Registered tokens resolve through subtree and UI overrides to theme
//! defaults, and bound properties follow them in Rust and in markup.
#![cfg(feature = "markup")]

use aegle::{
    Color, ColorSlot, LengthSlot, Node, Result, Size, TextSystem, Theme, ThemeOverride, Ui,
    UiError, Widgets, loader::Program, register_token, token,
};
use std::{cell::RefCell, rc::Rc};

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(320.0, 200.0))?;
    Ok(ui)
}

fn lane(theme: &Theme) -> Color {
    theme.accent
}

fn kind(error: Box<dyn std::error::Error>) -> Option<UiError> {
    error.downcast_ref::<UiError>().copied()
}

/// The bound background, radius and font size of `play` follow `lane`.
fn follows(ui: &Ui, lane: &Node, play: &Node) -> Result {
    let red = Color::rgb(200, 0, 0);
    let fill = token::<Color>("studio.lane")?;
    let size = token::<f32>("studio.type")?;
    assert_eq!(play.style()?.background, Some(Theme::light().accent));
    assert_eq!(play.style()?.radius, Some(0.0));
    ui.set_theme(Theme::dark())?;
    assert_eq!(play.style()?.background, Some(Theme::dark().accent));
    ui.set_token(fill, Some(Color::BLACK))?;
    lane.set_token(fill, Some(red))?;
    lane.set_token(Theme::RADIUS, Some(5.0))?;
    assert_eq!(play.style()?.background, Some(red));
    assert_eq!(play.style()?.radius, Some(5.0));
    assert_eq!(ui.token_value(fill)?, Color::BLACK);
    lane.set_token(fill, None)?;
    assert_eq!(play.style()?.background, Some(Color::BLACK));
    ui.set_token(size, Some(20.0))?;
    assert_eq!(play.token_value(size)?, 20.0);
    // A zero font size is refused and the old value restored.
    assert!(ui.set_token(size, Some(0.0)).is_err());
    assert_eq!(ui.token_value(size)?, 20.0);
    ui.set_token(fill, None)?;
    assert_eq!(play.style()?.background, Some(Theme::dark().accent));
    // A direct setter ends the binding.
    play.set_background(Color::WHITE)?;
    ui.set_token(fill, Some(red))?;
    assert_eq!(play.style()?.background, Some(Color::WHITE));
    Ok(())
}

#[test]
fn tokens_resolve_and_bindings_follow() -> Result {
    register_token("studio.lane", lane)?;
    register_token("studio.type", |theme| theme.font_size + 2.0)?;
    let again = register_token("studio.lane", |_| Color::WHITE)?;
    assert_eq!(again, token::<Color>("studio.lane")?);
    assert_eq!(token::<f32>("theme.radius")?, Theme::RADIUS);
    assert_eq!(
        kind(token::<f32>("studio.lane").unwrap_err()),
        Some(UiError::Token)
    );
    assert_eq!(
        kind(token::<Color>("studio.none").unwrap_err()),
        Some(UiError::Token)
    );
    for name in ["theme.extra", "nodot", "a..b", "a.b c"] {
        assert!(register_token(name, lane).is_err(), "{name}");
    }

    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/tokens.aegle")?;
    follows(&compiled, &view.lane, &view.play)?;

    let loaded = ui()?;
    let program = Program::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tokens.aegle"
    ))?;
    let runtime = program.build(&loaded.root())?;
    let lane = runtime.handle("lane").unwrap().node().clone();
    let play = runtime.handle("play").unwrap().node().clone();
    follows(&loaded, &lane, &play)?;

    // Moving into another subtree resolves against its overrides; removal
    // drops the subtree's overrides and bindings.
    let ui = ui()?;
    let fill = token::<Color>("studio.lane")?;
    let (left, right) = (ui.root().row()?, ui.root().row()?);
    right.set_token(fill, Some(Color::WHITE))?;
    let label = left.text("lane")?;
    label.bind_color(ColorSlot::Foreground, fill)?;
    label.bind_length(LengthSlot::FontSize, token("studio.type")?)?;
    assert!(label.bind_color(ColorSlot::Caret, fill).is_err());
    label.reparent(&right)?;
    assert_eq!(label.style()?.foreground, Some(Color::WHITE));
    label.unbind_token(ColorSlot::Foreground)?;
    assert_eq!(label.style()?.foreground, None);
    right.set_theme_override(Some(ThemeOverride {
        font_size: Some(30.0),
        ..Default::default()
    }))?;
    assert_eq!(label.token_value(token::<f32>("studio.type")?)?, 32.0);
    right.remove()?;
    Ok(())
}
