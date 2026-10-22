//! Tests for edge-docked Drawer navigation controls.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
use aegle_text::{Blob, GenericFamily};
use aegle_types::Point;
use aegle_ui::{Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::{DrawerSide, Widgets};

fn test_ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())
}

fn key(key: Key) -> KeyInput<'static> {
    KeyInput {
        key,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    }
}

#[test]
fn drawer_lifecycle_and_items() -> Result {
    let ui = test_ui()?;
    ui.resize(Size::new(800.0, 600.0));
    let root = ui.root();

    let drawer = root.drawer();
    let _item1 = drawer.item("Home");
    let _sep = drawer.separator();
    let _item2 = drawer.item("Settings");

    assert_eq!(drawer.side(), DrawerSide::Left);
    assert!(!drawer.is_open());
    assert!(!drawer.is_visible());

    let closed = Rc::new(Cell::new(false));
    let slot = closed.clone();
    let _sub = drawer.on_close(move || {
        slot.set(true);
    });

    // Open drawer
    drawer.open();
    assert!(drawer.is_open());
    assert!(drawer.is_visible());

    ui.refresh()?;

    // Change panel width
    drawer.set_width(320.0);

    // Toggle drawer to closed
    drawer.toggle();
    ui.dispatch_callbacks()?;
    assert!(!drawer.is_open());
    assert!(!drawer.is_visible());
    assert!(closed.get());

    // Toggle drawer back to open
    drawer.toggle();
    assert!(drawer.is_open());
    assert!(drawer.is_visible());

    Ok(())
}

#[test]
fn drawer_dismissal_and_escape() -> Result {
    let ui = test_ui()?;
    ui.resize(Size::new(800.0, 600.0));
    let root = ui.root();

    let drawer = root.drawer();
    drawer.item("Overview");

    drawer.open();
    ui.refresh()?;
    assert!(drawer.is_open());

    // Escape closes drawer
    ui.key(key(Key::Escape))?;
    ui.dispatch_callbacks()?;
    assert!(!drawer.is_open());

    // Reopen and test click outside on scrim (default width is 280.0, click at x=600)
    drawer.open();
    ui.refresh()?;
    assert!(drawer.is_open());

    let outside_pos = Point::new(600.0, 300.0);
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        outside_pos,
        Modifiers::default(),
    )?;
    ui.pointer(
        PointerId(1),
        PointerKind::Up,
        outside_pos,
        Modifiers::default(),
    )?;
    ui.dispatch_callbacks()?;
    assert!(!drawer.is_open());

    // Right-docked drawer
    let right_drawer = root.drawer_on(DrawerSide::Right);
    right_drawer.item("Inspector");
    assert_eq!(right_drawer.side(), DrawerSide::Right);

    right_drawer.open();
    ui.refresh()?;
    assert!(right_drawer.is_open());

    // Click at x=100 (left side, which is outside right drawer docked at 800 - 280 = 520)
    let left_pos = Point::new(100.0, 300.0);
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        left_pos,
        Modifiers::default(),
    )?;
    ui.pointer(
        PointerId(1),
        PointerKind::Up,
        left_pos,
        Modifiers::default(),
    )?;
    ui.dispatch_callbacks()?;
    assert!(!right_drawer.is_open());

    Ok(())
}
