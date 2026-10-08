//! Menus on popups: context menus, menus opened by a control, and menu bars.

use std::{ops::Deref, rc::Rc};

use aegle_core::{Dirty, NodeId};
use aegle_layout::{Dimension, FlexDirection, Style};
use aegle_ui::{Container, Control, Node, Result, State, text_style};

use crate::{
    NodePopup, Popup, Separator, Widgets,
    group::{Group, Role},
    menu_item::{MenuItem, MenuItemControl, item},
    popup::{entry, focus_first, hide_popup, popups, show_popup},
};

/// A popup of menu items. Up and Down move between items, Home and End to
/// the ends, Right opens a submenu (Left right to left), Left or Escape
/// closes it, Enter or a click chooses. The pointer resting on an item
/// focuses it and opens its submenu. Choosing an item closes every menu.
#[derive(Clone)]
pub struct Menu(Popup);

impl Deref for Menu {
    type Target = Popup;
    fn deref(&self) -> &Popup {
        &self.0
    }
}

impl Menu {
    fn new(anchor: &Node, side: bool) -> Result<Self> {
        let popup = anchor.popup()?;
        popup.change(|state, id| {
            state.control_as::<Group>(id).unwrap().role = Role::Menu;
            entry(state, id).side = side;
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })?;
        Ok(Self(popup))
    }
    /// Appends a command item.
    pub fn item(&self, text: &str) -> Result<MenuItem> {
        add_item(self, text, None, false)
    }
    /// Appends a check item; choosing it toggles the check before its
    /// handlers run.
    pub fn check_item(&self, text: &str, checked: bool) -> Result<MenuItem> {
        add_item(self, text, Some(checked), false)
    }
    /// Appends an item that opens a submenu beside it, and returns the
    /// submenu to fill.
    pub fn submenu(&self, text: &str) -> Result<Menu> {
        let item = add_item(self, text, None, false)?;
        let menu = Menu::new(&item, true)?;
        item.change(|state, id| {
            self::item(state, id).submenu = Some(menu.id);
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })?;
        Ok(menu)
    }
    /// Appends a horizontal divider between groups of items.
    pub fn separator(&self) -> Result<Separator> {
        self.0.0.separator()
    }
}

fn add_item(parent: &Container, text: &str, checked: Option<bool>, bar: bool) -> Result<MenuItem> {
    let node = crate::add(parent, |state, theme| {
        let control = MenuItemControl {
            button: aegle_controls::Button::new(),
            text: state
                .fonts
                .borrow_mut()
                .paragraph(text, &text_style(theme))?,
            checked,
            submenu: None,
            expanded: false,
            bar,
            handlers: Rc::default(),
        };
        let style = Style {
            size: aegle_layout::Size {
                width: Dimension::auto(),
                height: Dimension::length(theme.control_height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        };
        Ok((Box::new(control) as Box<dyn Control>, style))
    })?;
    node.change(|state, id| state.on_action(id, |node| chosen(MenuItem(node))))?;
    Ok(MenuItem(node))
}

/// Opens or toggles an item's menu, or closes every menu, toggles a check
/// item and runs its handlers.
fn chosen(item: MenuItem) -> Result {
    let handlers = item.change(|state, id| {
        let control = self::item(state, id);
        if let Some(menu) = control.submenu {
            let bar = control.bar;
            if bar && entry(state, menu).shown {
                hide_popup(state, menu)?;
            } else if entry(state, menu).shown {
                focus_first(state, menu)?;
            } else {
                show_popup(state, menu, true)?;
            }
            return Ok(None);
        }
        let handlers = control.handlers.clone();
        if let Some(checked) = &mut control.checked {
            *checked = !*checked;
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
        close_menus(state)?;
        Ok(Some(handlers))
    })?;
    let Some(handlers) = handlers else {
        return Ok(());
    };
    let mut callbacks = std::mem::take(&mut *handlers.borrow_mut());
    let result = crate::run_all(&mut callbacks, &item);
    let mut slot = handlers.borrow_mut();
    callbacks.append(&mut slot);
    *slot = callbacks;
    result
}

/// Hides every shown menu, the most recently shown first.
fn close_menus(state: &mut State) -> Result {
    let shown: Vec<_> = popups(state)
        .entries
        .iter()
        .filter(|e| e.shown)
        .map(|e| e.popup)
        .collect();
    for popup in shown.into_iter().rev() {
        if is_menu(state, popup) {
            hide_popup(state, popup)?;
        }
    }
    Ok(())
}

/// Menus on any control.
pub trait NodeMenu {
    /// Creates a hidden menu below this control, shown by [`Popup::show`],
    /// typically from a button's click handler.
    fn menu(&self) -> Result<Menu>;
    /// Creates a hidden menu shown where a context menu is requested over
    /// this control: at a secondary press, or at the focused control for
    /// the Menu key and Shift+F10 (see [`Node::on_context_menu`]).
    fn context_menu(&self) -> Result<Menu>;
}

impl NodeMenu for Node {
    fn menu(&self) -> Result<Menu> {
        Menu::new(self, false)
    }
    fn context_menu(&self) -> Result<Menu> {
        let menu = Menu::new(self, false)?;
        let shown = menu.clone();
        self.on_context_menu(move |_, at| shown.show_at(at))?;
        Ok(menu)
    }
}

/// A row of entries that each open a menu. Once one is open, resting on
/// another switches to it, and Left/Right move between them; on a focused
/// entry Left/Right move focus and Down opens it. F10 focuses the first
/// entry of the first menu bar.
#[derive(Clone)]
pub struct MenuBar(Container);

impl Deref for MenuBar {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.0
    }
}

impl MenuBar {
    /// Appends an entry opening a new menu below it, and returns the menu.
    pub fn menu(&self, text: &str) -> Result<Menu> {
        let item = add_item(&self.0, text, None, true)?;
        let menu = Menu::new(&item, false)?;
        item.change(|state, id| {
            self::item(state, id).submenu = Some(menu.id);
            Ok(())
        })?;
        Ok(menu)
    }
}

pub(crate) fn menu_bar(container: &Container) -> Result<MenuBar> {
    let bar = crate::group::add(container, Role::MenuBar, true)?;
    bar.change(|state, id| {
        popups(state).bars.push(id);
        let mut style = state.tree.get(id).unwrap().style().clone();
        style.flex_direction = FlexDirection::Row;
        style.flex_shrink = 0.0;
        aegle_layout::set_style(&mut state.tree, id, style)?;
        Ok(())
    })?;
    Ok(MenuBar(bar))
}

pub(crate) fn is_menu(state: &mut State, node: NodeId) -> bool {
    state
        .control_as::<Group>(node)
        .is_some_and(|g| g.role == Role::Menu)
}
