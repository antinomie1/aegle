//! Tests for modal Dialog controls.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use aegle_controls::{Key, KeyInput, Modifiers, PointerId, PointerKind};
use aegle_text::GenericFamily;
use aegle_types::Point;
use aegle_ui::{Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::Widgets;

fn test_ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
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
fn dialog_lifecycle_and_content() -> Result {
    let ui = test_ui()?;
    ui.resize(Size::new(800.0, 600.0));
    let root = ui.root();

    let dialog = root.dialog("Confirm Action");
    dialog.body().text("Are you sure you want to proceed?");
    let _ok_btn = dialog.button("OK");
    let _cancel_btn = dialog.button("Cancel");

    assert!(!dialog.is_open());
    assert!(!dialog.is_visible());

    let closed = Rc::new(Cell::new(false));
    let slot = closed.clone();
    let _sub = dialog.on_close(move || {
        slot.set(true);
    });

    // Show dialog
    dialog.show();
    assert!(dialog.is_open());
    assert!(dialog.is_visible());

    ui.refresh()?;

    // Change title
    dialog.set_title("Warning: High Risk");

    // Close dialog
    dialog.close();
    ui.dispatch_callbacks()?;
    assert!(!dialog.is_open());
    assert!(!dialog.is_visible());
    assert!(closed.get());

    Ok(())
}

#[test]
fn dialog_dismissal_and_escape() -> Result {
    let ui = test_ui()?;
    ui.resize(Size::new(800.0, 600.0));
    let root = ui.root();

    let dialog = root.dialog("Settings");
    dialog.body().text("Configure your workspace.");
    dialog.button("Save");

    dialog.show();
    ui.refresh()?;
    assert!(dialog.is_open());

    // Press Escape to dismiss
    ui.key(key(Key::Escape))?;
    ui.dispatch_callbacks()?;
    assert!(!dialog.is_open());

    // Show again and test click outside dismissal
    dialog.show();
    ui.refresh()?;
    assert!(dialog.is_open());

    // Click in top-left corner of window (outside the centered 320x... card)
    let outside_pos = Point::new(10.0, 10.0);
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
    assert!(!dialog.is_open());

    // Test non-dismissible dialog
    dialog.set_dismissible(false);
    dialog.show();
    ui.refresh()?;
    assert!(dialog.is_open());

    // Click outside should NOT close
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
    assert!(dialog.is_open());

    // Escape should NOT close when not dismissible
    ui.key(key(Key::Escape))?;
    ui.dispatch_callbacks()?;
    assert!(dialog.is_open());

    // Explicit close still works
    dialog.close();
    ui.dispatch_callbacks()?;
    assert!(!dialog.is_open());

    Ok(())
}
