//! Local styling shares retained text, composition, semantics and control behavior.
use aegle_app::{
    Appearance, Color, ImeEdit, Modifiers, Point, PointerId, PointerKind, Result, Size, Skin,
    Style, TextSystem, Theme, Ui,
};
use aegle_text::{Blob, GenericFamily, Selection, TextError};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn local_appearance_keeps_shared_state_and_font_overrides() -> Result {
    let skin: Skin = |theme, state| Appearance {
        background: Color::BLACK,
        ..Appearance::new(theme, state)
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
    assert!(ui.root().set_hover_background(Color::BLACK).is_err());
    assert!(field.set_pressed_background(Color::BLACK).is_err());
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
    field.set_skin(skin)?;
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
    button.set_skin(skin)?;
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
    ui.visit_scenes(|scene, _, _| {
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
    field.clear_font_size()?;
    field.set_style(Style::default())?;
    assert_eq!(field.appearance()?.background, Color::BLACK);
    field.clear_skin()?;
    assert_eq!(field.appearance()?.background, theme.surface);
    ui.refresh()?;
    ui.visit_scenes(|scene, _, _| {
        assert!(scene.glyph_runs().iter().all(|run| run.size() == 18.0));
        Ok(())
    })?;
    Ok(())
}
