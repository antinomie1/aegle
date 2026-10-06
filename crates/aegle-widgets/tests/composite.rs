//! Radio groups, mixed check boxes, popups, dropdowns, tables and content-sized rows.
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{
    Key, KeyInput, Modifiers, Point, PointerId, PointerKind, Result, Size, TextSystem, Theme, Ui,
};
use aegle_widgets::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(300.0, 400.0))?;
    Ok(ui)
}

fn key(ui: &Ui, key: Key) -> Result {
    let input = |pressed| KeyInput {
        key,
        text: "",
        modifiers: Modifiers::default(),
        pressed,
        repeat: false,
    };
    ui.key(input(true))?;
    ui.key(input(false))
}

#[test]
fn choices_popups_and_dropdowns() -> Result {
    let ui = ui()?;
    let group = ui.root().row()?;
    let a = group.radio("A", true)?;
    let b = group.radio("B", true)?; // A later checked radio wins.
    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    a.on_change(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    assert!(!a.is_checked()? && b.is_checked()?);
    a.toggle()?;
    a.toggle()?; // Choosing the chosen radio changes nothing.
    ui.dispatch_callbacks()?;
    assert!(a.is_checked()? && !b.is_checked()?);
    assert_eq!(changes.get(), 1);
    b.set_checked(true)?;
    assert!(!a.is_checked()? && changes.get() == 1);
    b.focus()?;
    key(&ui, Key::Right)?; // Arrows move the choice within the group, wrapping.
    ui.dispatch_callbacks()?;
    assert!(a.is_checked()? && a.visual_state()?.focused && changes.get() == 2);

    let all = ui.root().check_box("All", false)?;
    all.set_mixed(true)?;
    assert!(all.is_mixed()? && !all.visual_state()?.checked);
    all.toggle()?;
    assert!(!all.is_mixed()? && all.is_checked()?);

    let anchor = ui.root().button("Open")?;
    let behind = ui.root().button("Behind")?;
    let popup = anchor.popup()?;
    let first = popup.button("First")?;
    popup.button("Second")?;
    anchor.focus()?;
    ui.refresh()?;
    popup.show()?;
    ui.refresh()?;
    let (anchor_bounds, popup_bounds) = (anchor.bounds()?, popup.bounds()?);
    assert_eq!(
        popup_bounds.origin.y,
        anchor_bounds.origin.y + anchor_bounds.size.height
    );
    assert!(popup_bounds.size.width >= anchor_bounds.size.width);
    assert!(first.visual_state()?.focused);
    key(&ui, Key::Down)?;
    assert!(!first.visual_state()?.focused);
    key(&ui, Key::Escape)?;
    assert!(!popup.is_shown()? && anchor.visual_state()?.focused);
    // The popup covers what lies below it; a press elsewhere hides it.
    popup.show()?;
    ui.refresh()?;
    let over = behind.bounds()?.origin;
    let pointer = |kind, at: Point| ui.pointer(PointerId(1), kind, at, Modifiers::default());
    pointer(PointerKind::Move, Point::new(over.x + 1.0, over.y + 1.0))?;
    assert!(popup_bounds.contains(over) && !behind.visual_state()?.hovered);
    pointer(PointerKind::Down { clicks: 1 }, Point::new(290.0, 390.0))?;
    assert!(!popup.is_shown()?);

    let picked = Rc::new(Cell::new(None));
    let store = picked.clone();
    let dropdown = ui.root().dropdown(&["Red", "Green", "Blue"], 0)?;
    dropdown.on_change(move |d| {
        store.set(Some(d.selected()?));
        Ok(())
    })?;
    dropdown.focus()?;
    key(&ui, Key::Enter)?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    key(&ui, Key::Down)?;
    key(&ui, Key::Enter)?;
    ui.dispatch_callbacks()?;
    assert_eq!((dropdown.selected()?, picked.get()), (1, Some(1)));
    assert!(dropdown.visual_state()?.focused);
    dropdown.set_items(&["One", "Two"], 1)?;
    dropdown.set_selected(0)?;
    assert_eq!(picked.get(), Some(1)); // Programmatic selection is silent.
    assert!(dropdown.set_selected(5).is_err() && ui.root().dropdown(&[], 0).is_err());
    dropdown.remove()?;
    assert!(ui.refresh().is_ok());
    #[cfg(feature = "accessibility")]
    {
        use aegle_access::accesskit::{Role, Toggled};
        let tree = ui.accessibility(true, "")?;
        let role = |role| tree.nodes.iter().filter(|(_, n)| n.role() == role).count();
        assert_eq!((role(Role::RadioButton), role(Role::ListBox)), (2, 0));
        all.set_mixed(true)?;
        let tree = ui.accessibility(true, "")?;
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.toggled() == Some(Toggled::Mixed))
        );
    }
    Ok(())
}

#[test]
fn tables_and_content_sized_rows() -> Result {
    let ui = ui()?;
    ui.root().set_padding(0.0)?;
    let cells = Rc::new(RefCell::new(Vec::new()));
    let log = cells.clone();
    let columns = [
        TableColumn {
            title: "Name",
            width: Some(120.0),
        },
        TableColumn {
            title: "Size",
            width: None,
        },
    ];
    let table = ui
        .root()
        .table(&columns, 28.0, 1000, move |cell, row, column| {
            log.borrow_mut().push((row, column));
            cell.text(&format!("{row}:{column}")).map(drop)
        })?;
    table.set_height(Some(150.0))?;
    ui.refresh()?;
    assert_eq!(cells.borrow()[..2], [(0, 0), (0, 1)]);
    assert!(cells.borrow().len() < 20);
    assert_eq!(table.rows().count()?, 1000);
    assert!(ui.root().table(&[], 28.0, 1, |_, _, _| Ok(())).is_err());

    let list = ui.root().variable_list_view(20.0, 100, |row, index| {
        row.column()?
            .set_height(Some(if index % 2 == 0 { 50.0 } else { 10.0 }))
    })?;
    list.set_height(Some(200.0))?;
    ui.refresh()?;
    // Rows 0..=5 measured as 50, 10, 50, 10, 50, 10: the extent follows them.
    let extent = list.max_offset()?.y + 200.0;
    assert!(extent > 20.0 * 100.0 && extent < 20.0 * 100.0 + 6.0 * 30.0 + 1.0);
    list.scroll_to(Point::new(0.0, 60.0))?;
    ui.refresh()?;
    assert_eq!(list.offset()?.y, 60.0);
    list.set_count(3)?;
    ui.refresh()?;
    assert_eq!(list.max_offset()?.y, 0.0); // 50 + 10 + 50 is within the view.
    #[cfg(feature = "accessibility")]
    {
        use aegle_access::accesskit::Role;
        let tree = ui.accessibility(true, "")?;
        let role = |role| tree.nodes.iter().filter(|(_, n)| n.role() == role).count();
        assert_eq!((role(Role::Table), role(Role::ColumnHeader)), (1, 2));
        assert!(role(Role::Cell) >= 2);
    }
    Ok(())
}

#[test]
fn table_and_popup_surfaces_follow_theme_changes() -> Result {
    let ui = ui()?;
    let table = ui.root().table(
        &[TableColumn {
            title: "Name",
            width: None,
        }],
        28.0,
        10,
        |cell, row, _| cell.text(&format!("{row}")).map(drop),
    )?;
    table.set_height(Some(120.0))?;
    let popup = ui.root().button("Menu")?.popup()?;
    let (first, second) = (popup.text("One")?, popup.text("Two")?);
    ui.refresh()?;
    popup.show()?;
    for theme in [Theme::dark(), Theme::high_contrast()] {
        ui.set_theme(theme)?;
        ui.refresh()?;
        for panel in [table.appearance()?, popup.appearance()?] {
            assert_eq!(panel.background, theme.surface);
            assert_eq!(panel.border_color, theme.border);
        }
        // The popup keeps its zero gap and half-padding inset across themes.
        let (a, b, outer) = (first.bounds()?, second.bounds()?, popup.bounds()?);
        assert_eq!(b.origin.y, a.origin.y + a.size.height);
        assert_eq!(a.origin.y - outer.origin.y, theme.padding / 2.0);
    }
    Ok(())
}

#[test]
fn a_growing_table_shrinks_to_the_space_left() -> Result {
    let ui = ui()?;
    ui.resize(aegle_ui::Size::new(320.0, 240.0))?;
    // Like CSS flex, a container's minimum height follows its content; zero
    // lets this column shrink, and the table itself needs no such setting.
    let column = ui.root().column()?;
    column.set_grow(1.0)?;
    column.set_min_height(0.0)?;
    column.text("Above")?;
    let column_spec = [TableColumn {
        title: "Name",
        width: None,
    }];
    let table = column.table(&column_spec, 28.0, 1000, |cell, row, _| {
        cell.text(&format!("{row}")).map(drop)
    })?;
    table.set_grow(1.0)?;
    let below = ui.root().text("Below")?;
    ui.refresh()?;
    let (table, below) = (table.bounds()?, below.bounds()?);
    assert!(below.origin.y + below.size.height <= 240.0);
    assert!(table.size.height > 100.0 && table.origin.y + table.size.height <= below.origin.y);
    Ok(())
}
