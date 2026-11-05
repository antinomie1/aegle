//! Tooltips a host shows: reported once when due and once when gone, never
//! drawn in the window.
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use aegle_text::GenericFamily;
use aegle_ui::{Modifiers, Point, PointerId, PointerKind, Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::*;

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(200.0, 32.0));
    Ok(ui)
}

#[test]
fn host_shows_tooltips() -> Result {
    let ui = ui()?;
    host_tooltips(&ui)?;
    let button = ui.root().button("Volume");
    button.set_tooltip(Some("50 %"));
    ui.refresh()?;
    let (id, m) = (PointerId(1), Modifiers::default());
    let bounds = button.bounds();
    let center = Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    let children = || ui.root().change(|state, id| Ok(state.children(id).count()));
    let before = children();
    ui.pointer(id, PointerKind::Move, center, m)?;
    assert_eq!(take_tooltip(&ui), None);
    let due = ui.next_wake().expect("a pending tooltip");
    ui.wake(due + Duration::from_millis(1))?;
    ui.refresh()?;
    let shown = take_tooltip(&ui).flatten().expect("a tooltip to show");
    assert_eq!(shown.text, "50 %");
    assert_eq!(shown.anchor, bounds);
    assert_eq!(children(), before, "nothing drawn in the window");
    assert_eq!(take_tooltip(&ui), None, "reported once");
    ui.pointer(id, PointerKind::Move, center, m)?;
    assert_eq!(take_tooltip(&ui), None, "still shown");
    ui.pointer(id, PointerKind::Move, Point::new(199.0, 31.0), m)?;
    assert_eq!(take_tooltip(&ui), Some(None));
    Ok(())
}

#[test]
fn an_empty_tooltip_is_none() -> Result {
    let ui = ui()?;
    host_tooltips(&ui)?;
    let button = ui.root().button("Volume");
    button.set_tooltip(Some(""));
    ui.refresh()?;
    let bounds = button.bounds();
    let center = Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    ui.pointer(PointerId(1), PointerKind::Move, center, Modifiers::default())?;
    assert_eq!(ui.next_wake(), None, "nothing pending");
    assert_eq!(take_tooltip(&ui), None);
    Ok(())
}
