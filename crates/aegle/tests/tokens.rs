//! Registered tokens resolve through subtree and UI overrides to theme
//! defaults, and bound properties follow them in Rust and in markup.
#![cfg(feature = "markup")]

use aegle::{
    Color, ColorSlot, Easing, Font, LengthSlot, Node, Result, Size, TextSystem, Theme,
    ThemeOverride, TokenSlot, Transition, TransitionProperty::Paint, Ui, UiError, Widgets,
    loader::Program, register_token, token,
};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
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
    register_token("studio.space", |theme| theme.gap * 2.0)?;
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

fn serif(_: &Theme) -> Font {
    Font {
        families: "sans-serif",
        weight: 700,
        italic: false,
    }
}

#[test]
fn layout_font_and_transition_bindings_and_atomic_rejection() -> Result {
    let ui = ui()?;
    let space = register_token("theme-test.space", |theme| theme.gap * 2.0)?;
    let column = ui.root().column()?;
    let (first, second) = (column.text("a")?, column.text("b")?);
    column.bind_length(LengthSlot::Padding, space)?;
    column.bind_length(LengthSlot::Gap, space)?;
    let spacing = || -> Result<(f32, f32)> {
        ui.refresh()?;
        let (outer, a, b) = (column.bounds()?, first.bounds()?, second.bounds()?);
        Ok((
            a.origin.x - outer.origin.x,
            b.origin.y - a.origin.y - a.size.height,
        ))
    };
    assert_eq!(spacing()?, (16.0, 16.0));
    ui.set_token(space, Some(4.0))?;
    assert_eq!(spacing()?, (4.0, 4.0));
    column.unbind_token(LengthSlot::Padding)?;
    column.unbind_token(LengthSlot::Gap)?;
    assert_eq!(spacing()?, (0.0, Theme::light().gap));

    let face = register_token("theme-test.face", serif)?;
    first.bind_font(face)?;
    assert_eq!(first.font()?, Some(serif(&Theme::light())));
    let thin = Font {
        weight: 100,
        ..Font::DEFAULT
    };
    ui.set_token(face, Some(thin))?;
    assert_eq!(first.font()?, Some(thin));
    assert!(
        ui.set_token(face, Some(Font { weight: 0, ..thin }))
            .is_err()
    );
    assert!(column.bind_font(face).is_err());
    first.unbind_token(TokenSlot::Font)?;
    assert_eq!(first.font()?, None);

    let speed = register_token("theme-test.speed", |_| Duration::from_millis(120))?;
    first.bind_transition(Paint, speed, Easing::Linear)?;
    ui.set_token(speed, Some(Duration::from_millis(300)))?;
    let slow = Transition::new(Duration::from_millis(300), Easing::Linear);
    assert_eq!(first.property_transition(Paint)?, Some(slow));
    first.set_transition(Transition::default())?;
    ui.set_token(speed, None)?;
    assert_eq!(
        first.property_transition(Paint)?,
        Some(Transition::default())
    );

    // A theme change, override or move whose bound value is rejected (a font
    // size of 0 or less) is undone.
    let small = register_token("theme-test.small", |theme| theme.font_size - 16.0)?;
    let large = ThemeOverride {
        font_size: Some(20.0),
        ..Default::default()
    };
    column.set_theme_override(Some(large))?;
    first.bind_length(LengthSlot::FontSize, small)?;
    let plain = ui.root().row()?;
    assert!(first.reparent(&plain).is_err());
    assert_eq!(first.theme()?.font_size, 20.0);
    assert!(first.bounds()?.origin.y < second.bounds()?.origin.y);
    assert!(column.set_theme_override(None).is_err());
    assert!(column.set_theme(Some(Theme::light())).is_err());
    assert_eq!(column.theme()?.font_size, 20.0);
    let top = ui.root().text("top")?;
    let large = Theme {
        font_size: 20.0,
        ..Theme::light()
    };
    ui.set_theme(large)?;
    top.bind_length(LengthSlot::FontSize, small)?;
    assert!(ui.set_theme(Theme::dark()).is_err());
    assert_eq!(ui.theme(), large);
    assert_eq!(top.token_value(small)?, 4.0);
    Ok(())
}
