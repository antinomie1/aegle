//! A node that repaints, moves or resizes damages only its old and new
//! areas, including its shadow; structure changes damage the whole window.

use std::{cell::RefCell, rc::Rc};

use aegle_ui::{Color, Point, Rect, Result, Shadow, Size, TextSystem, Theme, Ui};

/// The damaged rectangles, or `None` for the whole window.
fn damage(ui: &Ui) -> Result<Option<Vec<Rect>>> {
    Ok(ui.damage()?.map(|d| d.rects().to_vec()))
}

#[test]
fn changes_damage_only_old_and_new_areas() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0))?;
    let panel = ui.root().column()?;
    panel.set_size(80.0, 40.0)?;
    panel.set_background(Color::BLACK)?;
    ui.refresh()?;
    assert_eq!(damage(&ui)?, None, "the first frame is whole");
    ui.clear_damage()?;

    panel.set_background(Color::WHITE)?;
    assert!(ui.refresh()?);
    let b = panel.bounds()?;
    assert_eq!(damage(&ui)?, Some(vec![b]));
    // Damage accumulates until a present clears it.
    assert!(!ui.refresh()?);
    assert_eq!(damage(&ui)?, Some(vec![b]));
    ui.clear_damage()?;

    let shadow = Shadow {
        offset: Point::new(0.0, 4.0),
        blur: 2.0,
        spread: 1.0,
        color: Color::rgba(0, 0, 0, 80),
    };
    panel.set_shadow(Some(shadow))?;
    ui.refresh()?;
    let reach = 1.0 + 2.0 * 3.0;
    let cast = Rect::new(
        b.origin.x - reach,
        b.origin.y + 4.0 - reach,
        b.size.width + reach * 2.0,
        b.size.height + reach * 2.0,
    );
    assert_eq!(damage(&ui)?, Some(vec![cast.union(b)]));
    ui.clear_damage()?;
    // Removing it repaints where the old shadow reached.
    panel.set_shadow(None)?;
    ui.refresh()?;
    assert_eq!(damage(&ui)?, Some(vec![cast.union(b)]));
    ui.clear_damage()?;

    // An unrelated node does not repaint when another one resizes.
    let other = ui.root().column()?;
    other.set_size(60.0, 20.0)?;
    other.set_background(Color::BLACK)?;
    ui.refresh()?;
    ui.clear_damage()?;
    panel.set_width(120.0)?;
    ui.refresh()?;
    assert_eq!(damage(&ui)?, Some(vec![b.union(panel.bounds()?)]));
    ui.clear_damage()?;

    ui.root().column()?;
    ui.refresh()?;
    assert_eq!(damage(&ui)?, None, "structure changes are whole");
    ui.clear_damage()?;
    // A new window background repaints areas no record covers.
    ui.set_theme(Theme::dark())?;
    ui.refresh()?;
    assert_eq!(damage(&ui)?, None);
    ui.clear_damage()?;
    ui.root().set_theme_override(Some(aegle_ui::ThemeOverride {
        background: Some(Color::rgb(10, 20, 30)),
        ..Default::default()
    }))?;
    ui.refresh()?;
    assert_eq!(damage(&ui)?, None);
    Ok(())
}
