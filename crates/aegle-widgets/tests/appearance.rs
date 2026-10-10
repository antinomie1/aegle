//! Local styling shares retained text, composition, semantics and control behavior.
use aegle_text::{Blob, GenericFamily, Selection, TextError};
use aegle_ui::{
    Appearance, Color, ImeEdit, Modifiers, Point, PointerId, PointerKind, Result, Size, Skin,
    Style, TextSystem, Theme, Ui,
};
use aegle_widgets::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn local_appearance_keeps_shared_state_and_font_overrides() -> Result {
    let skin: Skin = |theme, state| Appearance {
        background: Color::BLACK,
        ..Appearance::base(theme, state)
    };
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let field = ui.root().text_field("Hello")?;
    let button = ui.root().button("Apply")?;
    // Kind-specific setters exist only on matching handles; a Style value
    // is still checked against the control kind.
    let pressed = Style {
        pressed_background: Some(Color::BLACK),
        ..Default::default()
    };
    assert!(field.set_style(pressed).is_err());
    let hover = Style {
        hover_background: Some(Color::BLACK),
        ..Default::default()
    };
    assert!(ui.root().set_style(hover).is_err());
    ui.resize(Size::new(320.0, 160.0))?;
    field.focus()?;
    field.select(Selection {
        anchor: 5,
        focus: 5,
    })?;
    ui.refresh()?;
    ui.take_ime_state(4000)?;
    ui.ime(ImeEdit {
        preedit: "世界",
        ..Default::default()
    })?;
    let foreground = Color::rgb(9, 90, 40);
    field.set_skin(Some(skin))?;
    field.set_style(Style {
        background: Some(Color::WHITE),
        foreground: Some(foreground),
        border_width: Some(3.0),
        radius: Some(9.0),
        ..Default::default()
    })?;
    assert!(field.set_radius(f32::NAN).is_err());
    assert_eq!(field.style()?.radius, Some(9.0));
    field.set_font_size(22.0)?;
    let theme = Theme {
        font_size: 18.0,
        ..Theme::dark()
    };
    ui.set_theme(theme)?;
    ui.refresh()?;
    assert_eq!(field.text()?, "Hello");
    let ime = ui.take_ime_state(4000)?.unwrap();
    assert!(!ime.reset);
    assert_eq!(ime.request.unwrap().surrounding.as_deref(), Some("Hello"));
    let error = field.select(Selection::default()).unwrap_err();
    assert_eq!(error.downcast_ref(), Some(&TextError::CompositionActive));
    let appearance = field.appearance()?;
    assert_eq!(appearance.background, Color::WHITE);
    assert_eq!(
        (appearance.border_width, appearance.focus_width),
        (3.0, 2.0)
    );
    #[cfg(feature = "accessibility")]
    assert!(
        ui.accessibility(true, "Styled")?
            .nodes
            .iter()
            .any(|(_, node)| {
                node.foreground_color()
                    .is_some_and(|c| [c.red, c.green, c.blue, c.alpha] == foreground.to_rgba())
            })
    );
    ui.ime(ImeEdit {
        commit: Some("你好"),
        ..Default::default()
    })?;
    assert_eq!(field.text()?, "Hello你好");
    button.set_skin(Some(skin))?;
    button.set_style(Style {
        hover_background: Some(Color::WHITE),
        pressed_background: Some(foreground),
        disabled_background: Some(theme.border),
        ..Default::default()
    })?;
    ui.refresh()?;
    let bounds = button.bounds()?;
    let pointer = Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0);
    let point = |kind| ui.pointer(PointerId(1), kind, pointer, Modifiers::default());
    point(PointerKind::Move)?;
    assert_eq!(button.appearance()?.background, Color::WHITE);
    point(PointerKind::Down { clicks: 1 })?;
    assert_eq!(button.appearance()?.background, foreground);
    button.set_enabled(false)?;
    assert_eq!(button.appearance()?.background, theme.border);
    button.set_enabled(true)?;
    let target = field.clone();
    button.on_click(move |_| target.set_text("Done"))?;
    button.activate()?;
    ui.dispatch_callbacks()?;
    assert_eq!(field.text()?, "Done");
    ui.refresh()?;
    let mut local_runs = 0;
    ui.visit_scenes(|visit| {
        let aegle_ui::Visit::Scene { scene, .. } = visit else {
            return Ok(());
        };
        for run in scene
            .glyph_runs()
            .iter()
            .filter(|r| r.color() == foreground)
        {
            assert_eq!(run.size(), 22.0);
            local_runs += 1;
        }
        Ok(())
    })?;
    assert!(local_runs > 0);
    field.set_font_size(None)?;
    field.set_style(Style::default())?;
    assert_eq!(field.appearance()?.background, Color::BLACK);
    field.set_skin(None)?;
    assert_eq!(field.appearance()?.background, theme.surface);
    ui.refresh()?;
    ui.visit_scenes(|visit| {
        let aegle_ui::Visit::Scene { scene, .. } = visit else {
            return Ok(());
        };
        assert!(scene.glyph_runs().iter().all(|run| run.size() == 18.0));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn local_themes_inherit_nest_and_follow_reparenting() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    let tall = Theme {
        control_height: 50.0,
        ..Theme::dark()
    };
    let panel = ui.root().column()?;
    let inside = panel.button("")?;
    let outside = ui.root().button("")?;
    panel.set_theme(Some(tall))?;
    let later = panel.button("")?;
    let nested = panel.column()?;
    nested.set_theme(Some(Theme::high_contrast()))?;
    let deep = nested.button("")?;
    ui.resize(Size::new(200.0, 400.0))?;
    ui.refresh()?;
    assert_eq!(inside.bounds()?.size.height, 50.0);
    assert_eq!(later.bounds()?.size.height, 50.0);
    assert_eq!(inside.appearance()?.background, Theme::dark().surface);
    assert_eq!(deep.theme()?, Theme::high_contrast());
    assert_eq!(outside.theme()?, Theme::light());
    // The UI theme reaches only nodes without a local theme.
    ui.set_theme(Theme::dark())?;
    ui.refresh()?;
    assert_eq!(outside.appearance()?.background, Theme::dark().surface);
    assert_eq!(inside.theme()?, tall);
    outside.reparent(&nested)?;
    ui.refresh()?;
    assert_eq!(outside.theme()?, Theme::high_contrast());
    panel.set_theme(None)?;
    ui.refresh()?;
    assert_eq!(inside.bounds()?.size.height, 36.0);
    assert_eq!(deep.theme()?, Theme::high_contrast());
    assert!(panel.set_theme(Some(Theme { gap: -1.0, ..tall })).is_err());
    Ok(())
}

#[test]
fn token_overrides_follow_the_parent_theme() -> Result {
    use aegle_ui::ThemeOverride;
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    let panel = ui.root().column()?;
    let inner = panel.column()?;
    let button = inner.button("Go")?;
    let loud = ThemeOverride {
        accent: Some(Color::rgb(255, 0, 0)),
        gap: Some(2.0),
        ..Default::default()
    };
    panel.set_theme_override(Some(loud))?;
    let theme = button.theme()?;
    assert_eq!((theme.accent, theme.gap), (Color::rgb(255, 0, 0), 2.0));
    assert_eq!(theme.background, Theme::light().background);
    // Unset tokens keep following the UI theme; the set ones stay.
    ui.set_theme(Theme::dark())?;
    let theme = button.theme()?;
    assert_eq!(
        (theme.accent, theme.background),
        (Color::rgb(255, 0, 0), Theme::dark().background)
    );
    // A nested override layers over the outer one.
    inner.set_theme_override(Some(ThemeOverride {
        radius: Some(6.0),
        ..Default::default()
    }))?;
    let theme = button.theme()?;
    assert_eq!((theme.accent, theme.radius), (Color::rgb(255, 0, 0), 6.0));
    panel.set_theme_override(Some(ThemeOverride::default()))?;
    assert_eq!(button.theme()?.accent, Theme::dark().accent);
    assert_eq!(button.theme()?.radius, 6.0);
    assert!(
        panel
            .set_theme_override(Some(ThemeOverride {
                font_size: Some(0.0),
                ..Default::default()
            }))
            .is_err()
    );
    // A snapshot replaces the override; None restores the parent's theme.
    panel.set_theme(Some(Theme::high_contrast()))?;
    ui.set_theme(Theme::light())?;
    assert_eq!(
        button.theme()?.background,
        Theme::high_contrast().background
    );
    panel.set_theme(None)?;
    assert_eq!(button.theme()?.background, Theme::light().background);
    Ok(())
}

/// Skins for a kind cascade through subtrees like themes: the nearest
/// subtree's skin for the kind applies, a control's own skin wins, and
/// created or moved controls follow their new subtree.
#[test]
fn kind_skins_cascade_through_subtrees() -> Result {
    let outer: Skin = |theme, state| Appearance {
        background: theme.accent,
        ..Appearance::base(theme, state)
    };
    let inner: Skin = |_, state| Appearance {
        background: Color::BLACK,
        ..Appearance::base(&Theme::light(), state)
    };
    let own: Skin = |_, state| Appearance {
        background: Color::WHITE,
        ..Appearance::base(&Theme::light(), state)
    };
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    let root = ui.root();
    let panel = root.column()?;
    let nested = panel.column()?;
    let first = panel.button("first")?;
    let deep = nested.button("deep")?;
    let label = panel.text("label")?;
    let background = |node: &aegle_ui::Node| node.appearance().map(|a| a.background);
    let neutral = Theme::light().surface;
    assert_eq!(background(&first)?, neutral);

    root.set_kind_skin(&kinds::BUTTON, Some(outer))?;
    nested.set_kind_skin(&kinds::BUTTON, Some(inner))?;
    assert_eq!(background(&first)?, Theme::light().accent);
    assert_eq!(background(&deep)?, Color::BLACK);
    // Another kind is untouched; a skin follows the theme it is given.
    assert_eq!(background(&label)?, Color::TRANSPARENT);
    ui.set_theme(Theme::dark())?;
    assert_eq!(background(&first)?, Theme::dark().accent);

    // A control's own skin wins; without it the kind's skin applies again.
    first.set_skin(Some(own))?;
    assert_eq!(background(&first)?, Color::WHITE);
    first.set_skin(None)?;
    assert_eq!(background(&first)?, Theme::dark().accent);

    // Created and moved controls follow their subtree.
    let created = nested.button("created")?;
    assert_eq!(background(&created)?, Color::BLACK);
    first.reparent(&nested)?;
    assert_eq!(background(&first)?, Color::BLACK);
    deep.reparent(&panel)?;
    assert_eq!(background(&deep)?, Theme::dark().accent);

    // Removing a rule returns its subtree to the next one out.
    nested.set_kind_skin(&kinds::BUTTON, None)?;
    assert_eq!(background(&created)?, Theme::dark().accent);
    root.set_kind_skin(&kinds::BUTTON, None)?;
    assert_eq!(background(&created)?, Theme::dark().surface);
    Ok(())
}
