//! Paint-only changes damage just the changed node, including its shadow;
//! geometry and shadow changes damage the whole window until presented.

use std::{cell::RefCell, rc::Rc};

use aegle_ui::{Color, Point, Result, Shadow, Size, TextSystem, Theme, Ui};

#[test]
fn paint_changes_damage_only_their_node() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0))?;
    let panel = ui.root().column()?;
    panel.set_size(80.0, 40.0)?;
    ui.refresh()?;
    assert_eq!(ui.damage()?, None, "the first frame is whole");
    ui.clear_damage()?;

    panel.set_background(Color::BLACK)?;
    assert!(ui.refresh()?);
    assert_eq!(ui.damage()?, Some(panel.bounds()?));
    // Damage accumulates until a present clears it.
    assert!(!ui.refresh()?);
    assert_eq!(ui.damage()?, Some(panel.bounds()?));
    ui.clear_damage()?;

    let shadow = Shadow {
        offset: Point::new(0.0, 4.0),
        blur: 2.0,
        spread: 1.0,
        color: Color::rgba(0, 0, 0, 80),
    };
    panel.set_shadow(Some(shadow))?;
    ui.refresh()?;
    assert_eq!(ui.damage()?, None, "an old shadow may reach further");
    ui.clear_damage()?;
    panel.set_background(Color::WHITE)?;
    ui.refresh()?;
    let b = panel.bounds()?;
    let reach = 1.0 + 2.0 * 3.0;
    let cast = aegle_ui::Rect::new(
        b.origin.x - reach,
        b.origin.y + 4.0 - reach,
        b.size.width + reach * 2.0,
        b.size.height + reach * 2.0,
    );
    assert_eq!(ui.damage()?, Some(cast));
    ui.clear_damage()?;

    panel.set_width(120.0)?;
    ui.refresh()?;
    assert_eq!(ui.damage()?, None);
    Ok(())
}
