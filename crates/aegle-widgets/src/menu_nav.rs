//! Pointer and keyboard navigation of menus and menu bars.

use aegle_controls::{Key, KeyInput};
use aegle_core::NodeId;
use aegle_ui::{Result, State};

use crate::{
    menu::is_menu,
    menu_item::{MenuItemControl, item},
    popup::{controls, entry, focus_first, hide_popup, popups, show_popup},
};

/// The pointer resting on a menu item focuses it and shows its submenu,
/// hiding its siblings'; on a menu bar entry it switches the open menu.
pub(crate) fn hovered(state: &mut State, hit: Option<NodeId>) -> Result {
    let Some(id) = hit.filter(|&h| state.control_as::<MenuItemControl>(h).is_some()) else {
        return Ok(());
    };
    let parent = state.tree.parent(id)?.unwrap();
    let MenuItemControl { bar, submenu, .. } = *item(state, id);
    let open: Vec<_> = popups(state)
        .entries
        .iter()
        .filter(|e| e.shown && Some(e.popup) != submenu)
        .map(|e| (e.popup, e.anchor))
        .collect();
    let siblings: Vec<_> = open
        .into_iter()
        .filter(|&(_, anchor)| state.tree.parent(anchor).ok().flatten() == Some(parent))
        .map(|(popup, _)| popup)
        .collect();
    if bar {
        if let Some(&other) = siblings.first() {
            hide_popup(state, other)?;
            show_popup(state, submenu.unwrap(), true)?;
        }
        return Ok(());
    }
    if !(is_menu(state, parent) && entry(state, parent).shown) {
        return Ok(());
    }
    for other in siblings {
        hide_popup(state, other)?;
    }
    state.set_focus(Some(id))?;
    if let Some(menu) = submenu {
        show_popup(state, menu, false)?;
    }
    Ok(())
}

/// Menu and menu bar keys; returns whether the key was used.
pub(crate) fn menu_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    let plain = key.modifiers == aegle_controls::Modifiers::default();
    if key.key == Key::Function(10) && plain {
        let bar = popups(state).bars.first().copied();
        let first = bar.and_then(|bar| state.tree.children(bar).ok()?.next());
        if let Some(first) = first.filter(|&f| state.usable(f)) {
            state.set_focus(Some(first))?;
            return Ok(true);
        }
        return Ok(false);
    }
    let Some(id) = state.focus.current(&state.tree) else {
        return Ok(false);
    };
    let Some(&mut MenuItemControl { bar, submenu, .. }) = state.control_as::<MenuItemControl>(id)
    else {
        return Ok(false);
    };
    let parent = state.tree.parent(id)?.unwrap();
    let (forward, back) = if state.rtl(id) {
        (Key::Left, Key::Right)
    } else {
        (Key::Right, Key::Left)
    };
    if bar {
        let step = match key.key {
            Key::Down => {
                show_popup(state, submenu.unwrap(), true)?;
                return Ok(true);
            }
            k if k == forward => 1,
            k if k == back => -1,
            _ => return Ok(false),
        };
        let next = sibling(state, parent, id, step)?;
        state.set_focus(Some(next))?;
        return Ok(true);
    }
    if !is_menu(state, parent) {
        return Ok(false);
    }
    let items = controls(state, parent);
    match key.key {
        Key::Home | Key::End => {
            let end = if key.key == Key::Home {
                items.first()
            } else {
                items.last()
            };
            state.set_focus(end.copied())?;
            return Ok(true);
        }
        k if k == forward && submenu.is_some() => {
            let menu = submenu.unwrap();
            show_popup(state, menu, false)?;
            focus_first(state, menu)?;
            return Ok(true);
        }
        k if k == back => {
            if let Some(opener) = opener(state, parent) {
                hide_popup(state, parent)?;
                state.set_focus(Some(opener))?;
                return Ok(true);
            }
        }
        _ => {}
    }
    // Left/Right in a menu bar's menu move to the neighboring menu.
    let step = match key.key {
        k if k == forward => 1,
        k if k == back => -1,
        _ => return Ok(false),
    };
    let mut root = parent;
    while let Some(opener) = opener(state, root) {
        root = state.tree.parent(opener)?.unwrap();
    }
    let anchor = entry(state, root).anchor;
    if !is_bar_item(state, anchor) {
        return Ok(false);
    }
    let bar = state.tree.parent(anchor)?.unwrap();
    let next = sibling(state, bar, anchor, step)?;
    hide_popup(state, root)?;
    let menu = item(state, next).submenu.unwrap();
    show_popup(state, menu, true)?;
    Ok(true)
}

/// The menu item that opened `menu` as its submenu.
fn opener(state: &mut State, menu: NodeId) -> Option<NodeId> {
    let anchor = entry(state, menu).anchor;
    let parent = state.tree.parent(anchor).ok().flatten()?;
    (!is_bar_item(state, anchor) && is_menu(state, parent)).then_some(anchor)
}

fn is_bar_item(state: &mut State, node: NodeId) -> bool {
    state
        .control_as::<MenuItemControl>(node)
        .is_some_and(|item| item.bar)
}

/// The usable menu bar entry `step` places after `from`, wrapping.
fn sibling(state: &mut State, bar: NodeId, from: NodeId, step: isize) -> Result<NodeId> {
    let children: Vec<_> = state.tree.children(bar)?.collect();
    let entries: Vec<_> = children
        .into_iter()
        .filter(|&n| state.usable(n) && is_bar_item(state, n))
        .collect();
    let index = entries.iter().position(|&n| n == from).unwrap_or(0) as isize;
    let count = entries.len() as isize;
    Ok(entries[(index + step).rem_euclid(count) as usize])
}
