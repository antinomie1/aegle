//! The primary selection: what the focused editor selects is offered, and a
//! middle click on an editor puts the caret there and asks for it.
use std::{cell::RefCell, rc::Rc, sync::Arc};

use aegle_text::GenericFamily;
use aegle_ui::{
    Key, KeyInput, Modifiers, Point, PointerButton, PointerId, PointerKind, PrimaryRequest, Result,
    Size, TextSystem, Theme, Ui,
};
use aegle_widgets::*;

#[test]
fn editors_offer_and_paste_the_primary_selection() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(300.0, 100.0));
    let field = ui.root().text_field("hello");
    let secret = ui.root().text_field("hidden");
    secret.set_password(true);
    ui.refresh()?;
    let (id, m) = (PointerId(1), Modifiers::default());
    let center = |node: &aegle_ui::Node| {
        let b = node.bounds();
        Point::new(
            b.origin.x + b.size.width / 2.0,
            b.origin.y + b.size.height / 2.0,
        )
    };
    let select_all = |ui: &Ui| -> Result {
        let ctrl = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        for pressed in [true, false] {
            ui.key(KeyInput {
                key: Key::Character('a'),
                text: "",
                modifiers: ctrl,
                pressed,
                repeat: false,
            })?;
        }
        ui.refresh()?;
        Ok(())
    };
    ui.pointer(id, PointerKind::Down { clicks: 1 }, center(&field), m)?;
    ui.pointer(id, PointerKind::Up, center(&field), m)?;
    ui.refresh()?;
    while ui.take_primary().is_some() {}
    select_all(&ui)?;
    assert_eq!(
        ui.take_primary(),
        Some(PrimaryRequest::Write("hello".into()))
    );
    assert_eq!(ui.take_primary(), None);

    ui.pointer(
        id,
        PointerKind::ButtonDown(PointerButton::Middle),
        center(&secret),
        m,
    )?;
    ui.pointer(
        id,
        PointerKind::ButtonUp(PointerButton::Middle),
        center(&secret),
        m,
    )?;
    ui.refresh()?;
    assert_eq!(
        ui.take_primary(),
        Some(PrimaryRequest::Read),
        "a middle click asks"
    );
    assert!(secret.is_focused(), "and focuses the editor clicked");
    ui.paste(" world")?;
    assert_eq!(secret.text(), "hidden world", "pasted at the caret");
    select_all(&ui)?;
    assert_eq!(ui.take_primary(), None, "password fields offer nothing");
    Ok(())
}
