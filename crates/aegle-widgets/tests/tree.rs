//! Tests for hierarchical TreeView and TreeNode controls.

use std::{cell::RefCell, rc::Rc, sync::Arc};

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
fn tree_view_hierarchy_and_expansion() -> Result {
    let ui = test_ui()?;
    ui.resize(Size::new(400.0, 600.0));
    let root = ui.root();

    let tree = root.tree_view();
    let folder_a = tree.node("Folder A");
    let folder_b = tree.node("Folder B");
    let file_a1 = folder_a.node("File A1");
    let _file_a2 = folder_a.node("File A2");
    let file_b1 = folder_b.node("File B1");

    assert_eq!(folder_a.text(), "Folder A");
    folder_a.set_text("Project Root");
    assert_eq!(folder_a.text(), "Project Root");

    // Initially collapsed
    assert!(!folder_a.is_expanded());
    assert!(!folder_b.is_expanded());

    ui.refresh()?;

    // Children column should be hidden when collapsed
    let children_col = folder_a.children_container();
    assert!(!children_col.is_visible());

    // Expand folder_a
    folder_a.set_expanded(true);
    assert!(folder_a.is_expanded());
    assert!(children_col.is_visible());

    // Toggle back to collapsed
    folder_a.toggle();
    assert!(!folder_a.is_expanded());
    assert!(!children_col.is_visible());

    // Selection state
    assert!(!file_a1.is_selected());
    file_a1.set_selected(true);
    assert!(file_a1.is_selected());
    file_a1.set_selected(false);
    assert!(!file_a1.is_selected());

    // Callback on selection
    let selected_label = Rc::new(RefCell::new(String::new()));
    let slot = selected_label.clone();
    let _sub = file_b1.on_select(move |n| {
        *slot.borrow_mut() = n.text();
    });

    Ok(())
}

#[test]
fn tree_node_key_and_click_interaction() -> Result {
    let ui = test_ui()?;
    ui.resize(Size::new(400.0, 600.0));
    let root = ui.root();

    let tree = root.tree_view();
    let folder = tree.node("Docs");
    let _child = folder.node("Guide.md");

    ui.refresh()?;
    folder.focus();

    // Arrow Right expands folder
    ui.key(key(Key::Right))?;
    ui.dispatch_callbacks()?;
    assert!(folder.is_expanded());
    assert!(folder.children_container().is_visible());

    // Arrow Left collapses folder
    ui.key(key(Key::Left))?;
    ui.dispatch_callbacks()?;
    assert!(!folder.is_expanded());
    assert!(!folder.children_container().is_visible());

    // Pointer click on chevron toggles
    let bounds = folder.bounds();
    // Chevron is at the far left
    let chevron_pos = Point::new(
        bounds.origin.x + 8.0,
        bounds.origin.y + bounds.size.height * 0.5,
    );
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        chevron_pos,
        Modifiers::default(),
    )?;
    ui.pointer(
        PointerId(1),
        PointerKind::Up,
        chevron_pos,
        Modifiers::default(),
    )?;
    ui.dispatch_callbacks()?;
    assert!(folder.is_expanded());

    // Click label area selects
    let label_pos = Point::new(
        bounds.origin.x + 80.0,
        bounds.origin.y + bounds.size.height * 0.5,
    );
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        label_pos,
        Modifiers::default(),
    )?;
    ui.pointer(
        PointerId(1),
        PointerKind::Up,
        label_pos,
        Modifiers::default(),
    )?;
    ui.dispatch_callbacks()?;
    assert!(folder.is_selected());

    Ok(())
}
