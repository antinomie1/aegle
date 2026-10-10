//! Context menus, submenus, check and radio items, shortcut hints and menu bars.
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{
    Key, KeyInput, Modifiers, Point, PointerButton, PointerId, PointerKind, Rect, Result, Size,
    TextSystem, Theme, Ui,
};
use aegle_widgets::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(400.0, 300.0))?;
    Ok(ui)
}

fn key_with(ui: &Ui, key: Key, modifiers: Modifiers) -> Result {
    let input = |pressed| KeyInput {
        key,
        text: "",
        modifiers,
        pressed,
        repeat: false,
    };
    ui.key(input(true))?;
    ui.key(input(false))?;
    ui.dispatch_callbacks()?;
    ui.refresh().map(drop)
}

fn key(ui: &Ui, key: Key) -> Result {
    key_with(ui, key, Modifiers::default())
}

fn center(bounds: Rect) -> Point {
    Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    )
}

fn click(ui: &Ui, at: Point) -> Result {
    let none = Modifiers::default();
    ui.pointer(PointerId(1), PointerKind::Down { clicks: 1 }, at, none)?;
    ui.pointer(PointerId(1), PointerKind::Up, at, none)?;
    ui.dispatch_callbacks()?;
    ui.refresh().map(drop)
}

fn hover(ui: &Ui, at: Point) -> Result {
    ui.pointer(PointerId(1), PointerKind::Move, at, Modifiers::default())?;
    ui.refresh().map(drop)
}

fn focused(item: &MenuItem) -> Result<bool> {
    item.is_focused()
}

#[test]
fn context_menus_open_where_requested_and_close_on_choice() -> Result {
    let ui = ui()?;
    let area = ui.root().button("Area")?;
    area.set_width(380.0)?;
    area.set_height(280.0)?;
    let menu = area.context_menu()?;
    let log = Rc::new(RefCell::new(Vec::new()));
    let copy = menu.item("Copy")?;
    let seen = log.clone();
    copy.on_click(move |_| {
        seen.borrow_mut().push("copy");
        Ok(())
    })?;
    menu.separator()?;
    let wrap = menu.check_item("Wrap", false)?;
    let seen = log.clone();
    wrap.on_click(move |item| {
        seen.borrow_mut()
            .push(if item.is_checked()? { "on" } else { "off" });
        Ok(())
    })?;
    ui.refresh()?;

    // A secondary press shows the menu with its corner at the point.
    let at = Point::new(120.0, 80.0);
    let secondary = PointerKind::ButtonDown(PointerButton::Secondary);
    ui.pointer(PointerId(1), secondary, at, Modifiers::default())?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    assert!(menu.is_shown()?);
    assert_eq!(menu.bounds()?.origin, at);
    assert!(focused(&copy)?);
    // Down skips the separator; Enter toggles the check item and closes.
    key(&ui, Key::Down)?;
    assert!(focused(&wrap)?);
    key(&ui, Key::Enter)?;
    assert!(!menu.is_shown()?);
    assert!(wrap.is_checked()?);

    // Near the window's corner it flips to fit; a press on the anchor area
    // outside the menu closes it.
    let corner = Point::new(370.0, 270.0);
    ui.pointer(PointerId(1), secondary, corner, Modifiers::default())?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    let bounds = menu.bounds()?;
    assert_eq!(bounds.origin.x + bounds.size.width, corner.x);
    assert!(bounds.origin.y + bounds.size.height <= 300.0);
    click(&ui, Point::new(20.0, 20.0))?;
    assert!(!menu.is_shown()?);

    // The Menu key and Shift+F10 open it at the focused control.
    area.focus()?;
    key(&ui, Key::ContextMenu)?;
    assert!(menu.is_shown()?);
    assert_eq!(menu.bounds()?.origin, area.bounds()?.origin);
    key(&ui, Key::Escape)?;
    assert!(!menu.is_shown()?);
    assert!(area.visual_state()?.focused);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    key_with(&ui, Key::Function(10), shift)?;
    assert!(menu.is_shown()?);
    click(&ui, center(copy.bounds()?))?;
    assert!(!menu.is_shown()?);
    assert_eq!(*log.borrow(), ["on", "copy"]);

    #[cfg(feature = "accessibility")]
    {
        use aegle_access::accesskit::{Action, ActionRequest, Role, Toggled, TreeId};
        let tree = ui.accessibility(true, "Menus")?;
        let find = |role| {
            tree.nodes
                .iter()
                .find(|(_, node)| node.role() == role)
                .unwrap()
        };
        let (target, button) = find(Role::Button);
        assert!(button.supports_action(Action::ShowContextMenu));
        assert!(find(Role::Menu).1.is_hidden());
        let check = &find(Role::MenuItemCheckBox).1;
        assert_eq!(check.toggled(), Some(Toggled::True));
        let request = ActionRequest {
            action: Action::ShowContextMenu,
            target_tree: TreeId::ROOT,
            target_node: *target,
            data: None,
        };
        assert!(ui.access_action(request)?);
        ui.dispatch_callbacks()?;
        assert!(menu.is_shown()?);
    }
    Ok(())
}

#[test]
fn submenus_follow_hover_and_arrow_keys() -> Result {
    let ui = ui()?;
    let button = ui.root().row()?.button("File")?;
    let menu = button.menu()?;
    let new = menu.item("New")?;
    let recent = menu.submenu("Recent")?;
    let first = recent.item("a.txt")?;
    recent.item("b.txt")?;
    let quit = menu.item("Quit")?;
    ui.refresh()?;
    menu.show()?;
    ui.refresh()?;
    assert!(focused(&new)?);

    // Right opens the submenu beside its item and focuses its first entry;
    // Left closes it and returns to the item.
    key(&ui, Key::Down)?;
    key(&ui, Key::Right)?;
    assert!(recent.is_shown()?);
    assert!(focused(&first)?);
    let (opener, inner) = (recent.anchor()?.bounds()?, recent.bounds()?);
    assert_eq!(inner.origin.x, opener.origin.x + opener.size.width);
    assert_eq!(inner.origin.y, opener.origin.y);
    key(&ui, Key::Left)?;
    assert!(!recent.is_shown()?);
    assert!(menu.is_shown()?);
    key(&ui, Key::End)?;
    assert!(focused(&quit)?);

    // Resting on the opener shows the submenu without moving focus into it;
    // resting on a sibling hides it again.
    let opener = center(recent.anchor()?.bounds()?);
    hover(&ui, opener)?;
    assert!(recent.is_shown()?);
    assert!(!focused(&first)?);
    hover(&ui, center(quit.bounds()?))?;
    assert!(!recent.is_shown()?);
    assert!(focused(&quit)?);

    // Hiding a menu hides its open submenu.
    hover(&ui, opener)?;
    assert!(recent.is_shown()?);
    menu.hide()?;
    assert!(!recent.is_shown()?);
    Ok(())
}

#[test]
fn menu_bars_switch_between_open_menus() -> Result {
    let ui = ui()?;
    let bar = ui.root().menu_bar()?;
    let file = bar.menu("File")?;
    let open = file.item("Open")?;
    let edit = bar.menu("Edit")?;
    let undo = edit.item("Undo")?;
    ui.root().button("Body")?;
    ui.refresh()?;
    let file_entry = center(file.anchor()?.bounds()?);
    let edit_entry = center(edit.anchor()?.bounds()?);

    // A click opens a menu below its entry; resting on the next entry
    // switches menus; a second click on that entry closes it.
    click(&ui, file_entry)?;
    assert!(file.is_shown()?);
    assert!(focused(&open)?);
    hover(&ui, edit_entry)?;
    assert!(!file.is_shown()? && edit.is_shown()?);
    click(&ui, edit_entry)?;
    assert!(!edit.is_shown()?);

    // F10 focuses the first entry; Right moves along the bar and Down
    // opens; inside a menu Left and Right move to the neighboring menu.
    key(&ui, Key::Function(10))?;
    key(&ui, Key::Right)?;
    key(&ui, Key::Down)?;
    assert!(edit.is_shown()? && focused(&undo)?);
    key(&ui, Key::Right)?;
    assert!(!edit.is_shown()? && file.is_shown()? && focused(&open)?);
    key(&ui, Key::Left)?;
    assert!(edit.is_shown()?);
    key(&ui, Key::Escape)?;
    assert!(!edit.is_shown()?);
    Ok(())
}

#[test]
fn radio_items_are_exclusive_within_their_run() -> Result {
    let ui = ui()?;
    let button = ui.root().row()?.button("View")?;
    let menu = button.menu()?;
    let small = menu.radio_item("Small", true)?;
    let large = menu.radio_item("Large", false)?;
    menu.separator()?;
    let light = menu.radio_item("Light", true)?;
    let save = menu.item("Save")?;
    menu.show()?;
    // The hint at the end of the item widens the menu, and follows the
    // theme's font size like the item's text.
    let share = |ui: &Ui| -> Result<f32> {
        ui.refresh()?;
        let width = menu.bounds()?.size.width;
        save.set_shortcut(Some("Ctrl+S"))?;
        ui.refresh()?;
        Ok(menu.bounds()?.size.width - width)
    };
    let normal = share(&ui)?;
    assert!(normal > 20.0);
    let theme = Theme::light();
    save.set_shortcut(None)?;
    ui.set_theme(Theme {
        font_size: theme.font_size * 2.0,
        ..theme
    })?;
    let doubled = share(&ui)?;
    assert!(doubled > normal * 1.5, "{normal} → {doubled}");
    ui.set_theme(theme)?;
    ui.refresh()?;

    // Choosing a radio item unchecks only its own run; choosing it again
    // keeps it checked.
    click(&ui, center(large.bounds()?))?;
    assert!(large.is_checked()? && !small.is_checked()? && light.is_checked()?);
    menu.show()?;
    ui.refresh()?;
    click(&ui, center(large.bounds()?))?;
    assert!(large.is_checked()?);
    small.set_checked(true)?;
    assert!(small.is_checked()? && !large.is_checked()? && light.is_checked()?);

    #[cfg(feature = "accessibility")]
    {
        use aegle_access::accesskit::{Role, Toggled};
        let tree = ui.accessibility(true, "Menus")?;
        let radios: Vec<_> = (tree.nodes.iter())
            .filter(|(_, node)| node.role() == Role::MenuItemRadio)
            .map(|(_, node)| node.toggled())
            .collect();
        assert_eq!(
            radios,
            [
                Some(Toggled::True),
                Some(Toggled::False),
                Some(Toggled::True)
            ]
        );
        let hint = tree
            .nodes
            .iter()
            .find_map(|(_, node)| node.keyboard_shortcut());
        assert_eq!(hint, Some("Ctrl+S"));
    }
    Ok(())
}
