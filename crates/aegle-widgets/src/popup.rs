//! In-window popups and the dropdown built on them.

use std::{collections::HashMap, ops::Deref};

use aegle_controls::{Key, KeyInput};
use aegle_core::{Dirty, NodeId};
use aegle_layout::{Edges, LengthPercentageAuto, Position};
use aegle_types::{Point, Rect};
use aegle_ui::{Container, Control, Node, Result, State, container_style};

use crate::{
    button::Variant,
    dropdown::DropdownData,
    group::{Group, Role},
};

pub(crate) struct PopupEntry {
    pub popup: NodeId,
    pub anchor: NodeId,
    pub shown: bool,
    /// Focus to restore when the popup closes with focus inside it.
    pub restore: Option<NodeId>,
    /// Placed beside the anchor (a submenu) rather than below it.
    pub side: bool,
    /// The window point it was shown at, overriding the anchor placement.
    pub at: Option<Point>,
}

/// Popups and dropdowns of one UI, kept in the engine's per-library storage.
#[derive(Default)]
pub(crate) struct Popups {
    /// Shown or hidden popups with their anchors, in showing order.
    pub entries: Vec<PopupEntry>,
    /// Dropdown anchors and their choices.
    pub dropdowns: HashMap<NodeId, DropdownData>,
    /// Menu bars in creation order.
    pub bars: Vec<NodeId>,
}

pub(crate) fn popups(state: &mut State) -> &mut Popups {
    state.ext()
}

/// A floating column drawn above every other control of its window, below its
/// anchor control, or above it when only that fits. It does not take layout
/// space. A press outside it and its anchor, or Escape, hides it; Up and Down
/// move focus among its controls. Removing the anchor removes the popup.
#[derive(Clone)]
pub struct Popup(pub(crate) Container);

impl Deref for Popup {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.0
    }
}

pub(crate) fn popup(anchor: &Node) -> Popup {
    let id = anchor.change(|state, anchor| {
        state.install(&crate::HOOKS);
        let theme = *state.theme_of(anchor);
        let mut style = container_style(&theme, false);
        style.position = Position::Absolute;
        style.inset = Edges {
            left: LengthPercentageAuto::length(0.0),
            top: LengthPercentageAuto::length(0.0),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        };
        let root = state.root;
        let group = Group {
            role: Role::Popup { list: false },
        };
        group.retheme(&theme, aegle_ui::LocalLayout::NONE, false, &mut style);
        let id = state.insert(root, usize::MAX, Box::new(group), style)?;
        state.set_visible(id, false)?;
        let anchor_element = &state.tree.get(anchor).unwrap().context;
        if anchor_element.theme.is_some() {
            let local = anchor_element.theme.clone();
            state.propagate_theme(id, local)?;
        }
        popups(state).entries.push(PopupEntry {
            popup: id,
            anchor,
            shown: false,
            restore: None,
            side: false,
            at: None,
        });
        Ok(id)
    });
    Popup(Container(Node {
        state: anchor.state.clone(),
        id,
    }))
}

impl Popup {
    /// Shows the popup above all controls, at least as wide as its anchor,
    /// and focuses its first enabled control.
    pub fn show(&self) {
        self.change(|state, id| {
            entry(state, id).at = None;
            show_popup(state, id, true)
        })
    }
    /// Shows the popup with its top start corner at a logical window point,
    /// moved or flipped to fit the window, and focuses its first enabled
    /// control; a shown popup moves there. Used for context menus: a press
    /// on the anchor then hides it too.
    pub fn show_at(&self, at: Point) {
        if !(at.x.is_finite() && at.y.is_finite()) {
            panic!("{}", aegle_ui::UiError::InvalidValue);
        }
        self.change(|state, id| {
            entry(state, id).at = Some(at);
            state.geometry_dirty = true;
            show_popup(state, id, true)
        })
    }
    /// Hides the popup and every popup anchored inside it, returning focus
    /// to where it was when shown.
    pub fn hide(&self) {
        self.change(hide_popup)
    }
    /// The control the popup is anchored to: for a submenu its item, for a
    /// menu bar's menu its entry.
    pub fn anchor(&self) -> Node {
        let id = self.change(|state, id| Ok(entry(state, id).anchor));
        Node {
            state: self.state.clone(),
            id,
        }
    }
    /// Whether the popup is currently shown.
    pub fn is_shown(&self) -> bool {
        self.change(|state, id| {
            Ok(popups(state)
                .entries
                .iter()
                .any(|e| e.popup == id && e.shown))
        })
    }
}

pub(crate) fn entry(state: &mut State, popup: NodeId) -> &mut PopupEntry {
    popups(state)
        .entries
        .iter_mut()
        .find(|e| e.popup == popup)
        .unwrap()
}

fn set_expanded(state: &mut State, anchor: NodeId, expanded: bool) {
    if let Some(button) = state.control_as::<crate::button::ButtonControl>(anchor)
        && let Variant::Dropdown { expanded: old } = &mut button.variant
    {
        *old = expanded;
    }
    if let Some(item) = state.control_as::<crate::MenuItemControl>(anchor) {
        item.expanded = expanded;
    }
}

/// Shows a popup, focusing its first enabled control if `focus`.
pub(crate) fn show_popup(state: &mut State, id: NodeId, focus: bool) -> Result {
    if entry(state, id).shown {
        return Ok(());
    }
    // The root's last child paints over, and is hit before, everything else.
    let root = state.root;
    state.tree.reparent(id, Some(root))?;
    state.invalidate_structure();
    let PopupEntry {
        anchor, side, at, ..
    } = *entry(state, id);
    let mut style = state.tree.get(id).unwrap().style().clone();
    style.min_size.width = if side || at.is_some() {
        LengthPercentageAuto::auto()
    } else {
        let width = state.tree.get(anchor).unwrap().context.bounds.size.width;
        LengthPercentageAuto::length(width)
    };
    aegle_layout::set_style(&mut state.tree, id, style)?;
    // It lives under the root but reads in its anchor's direction.
    let direction = state.tree.get(anchor).unwrap().style().direction;
    state.tree.get_mut(id).unwrap().context.direction = Some(direction);
    state.propagate_direction(id)?;
    state.set_visible(id, true)?;
    let restore = state.focus.current(&state.tree);
    let e = entry(state, id);
    e.shown = true;
    e.restore = restore;
    set_expanded(state, anchor, true);
    state.tree.mark_dirty(anchor, Dirty::SEMANTICS)?;
    state.rebuild_order();
    if focus {
        focus_first(state, id)?;
    }
    Ok(())
}

/// Focuses the first enabled control of a popup, if any.
pub(crate) fn focus_first(state: &mut State, popup: NodeId) -> Result {
    let first = controls(state, popup).first().copied();
    if first.is_some() {
        state.set_focus(first)?;
    }
    Ok(())
}

/// The usable interactive controls of a popup in focus order.
pub(crate) fn controls(state: &mut State, popup: NodeId) -> Vec<NodeId> {
    state.rebuild_order();
    state
        .order
        .iter()
        .copied()
        .filter(|&n| {
            n != popup
                && state.contains(popup, n)
                && state.tree.get(n).unwrap().context.control.interactive()
                && state.usable(n)
        })
        .collect()
}

pub(crate) fn hide_popup(state: &mut State, id: NodeId) -> Result {
    if !entry(state, id).shown {
        return Ok(());
    }
    let nested: Vec<_> = popups(state)
        .entries
        .iter()
        .filter(|e| e.shown)
        .map(|e| (e.popup, e.anchor))
        .collect();
    for (popup, anchor) in nested.into_iter().rev() {
        if state.contains(id, anchor) {
            hide_popup(state, popup)?;
        }
    }
    let e = entry(state, id);
    e.shown = false;
    let (anchor, restore) = (e.anchor, e.restore.take());
    let inside = state
        .focus
        .current(&state.tree)
        .is_some_and(|f| state.contains(id, f));
    state.set_visible(id, false)?;
    if inside && restore.is_some_and(|r| state.usable(r)) {
        state.set_focus(restore)?;
    }
    set_expanded(state, anchor, false);
    state.tree.mark_dirty(anchor, Dirty::SEMANTICS)?;
    Ok(())
}

/// Places shown popups by their anchors after geometry; returns whether any moved.
pub(crate) fn place_popups(state: &mut State) -> bool {
    let mut moved = false;
    let shown: Vec<_> = match state.ext_ref::<Popups>() {
        Some(p) => p
            .entries
            .iter()
            .filter(|e| e.shown)
            .map(|e| (e.popup, e.anchor, e.side, e.at))
            .collect(),
        None => return false,
    };
    // Prefers `preferred` for a span `size` within `0..limit`, else
    // `fallback`, then clamps into the window.
    let fit = |preferred: f32, fallback: f32, size: f32, limit: f32| {
        let fits = |start: f32| start >= 0.0 && start + size <= limit;
        let start = if fits(preferred) || !fits(fallback) {
            preferred
        } else {
            fallback
        };
        start.min(limit - size).max(0.0)
    };
    let window = state.size;
    for (popup, anchor, side, at) in shown {
        let rtl = state.rtl(anchor);
        let anchor = state.tree.get(anchor).unwrap().context.bounds;
        let size = state.tree.get(popup).unwrap().bounds().size;
        let (x, y) = if let Some(at) = at {
            // The start corner at the point, else the end corner.
            let (start, end) = (at.x, at.x - size.width);
            let (first, second) = if rtl { (end, start) } else { (start, end) };
            let x = fit(first, second, size.width, window.width);
            let y = fit(at.y, at.y - size.height, size.height, window.height);
            (x, y)
        } else if side {
            // Beside the anchor on its end side, else its start side; level
            // with its top, else with its bottom.
            let after = anchor.origin.x + anchor.size.width;
            let before = anchor.origin.x - size.width;
            let (first, second) = if rtl {
                (before, after)
            } else {
                (after, before)
            };
            let x = fit(first, second, size.width, window.width);
            let bottom = anchor.origin.y + anchor.size.height - size.height;
            let y = fit(anchor.origin.y, bottom, size.height, window.height);
            (x, y)
        } else {
            // Aligned with the anchor's start edge: its right edge right to left.
            let start = if rtl {
                anchor.origin.x + anchor.size.width - size.width
            } else {
                anchor.origin.x
            };
            let x = start.min(window.width - size.width).max(0.0);
            let below = anchor.origin.y + anchor.size.height;
            let above = anchor.origin.y - size.height;
            let y = if below + size.height <= window.height || above < 0.0 {
                below
            } else {
                above
            };
            (x, y)
        };
        let element = &mut state.tree.get_mut(popup).unwrap().context;
        if element.offset != Point::new(x, y) {
            element.offset = Point::new(x, y);
            moved = true;
        }
    }
    moved
}

/// The topmost shown popup containing a window point, if any.
pub(crate) fn popup_at(state: &State, position: Point) -> Option<NodeId> {
    let entries = &state.ext_ref::<Popups>()?.entries;
    entries
        .iter()
        .rev()
        .filter(|e| e.shown)
        .map(|e| e.popup)
        .find(|&p| state.tree.get(p).unwrap().context.bounds.contains(position))
}

/// Hides shown popups unless a press lands in them, their anchor, or a
/// popup anchored inside them.
pub(crate) fn dismiss_popups(state: &mut State, position: Point) -> Result {
    let Some(shown) = state.ext_ref::<Popups>().map(|p| {
        p.entries
            .iter()
            .filter(|e| e.shown)
            .map(|e| (e.popup, e.anchor, e.at))
            .collect::<Vec<_>>()
    }) else {
        return Ok(());
    };
    let inside = |state: &State, node: NodeId| {
        let bounds: Rect = state.tree.get(node).unwrap().context.bounds;
        bounds.contains(position)
    };
    let hit = popup_at(state, position);
    let mut keep = Vec::new();
    for &(popup, anchor, at) in &shown {
        let nested = hit.is_some_and(|hit| {
            let hit_anchor = state
                .ext_ref::<Popups>()
                .and_then(|p| p.entries.iter().find(|e| e.popup == hit))
                .unwrap()
                .anchor;
            state.contains(popup, hit_anchor)
        });
        // A popup shown at a point is not attached to its anchor's area.
        let on_anchor = at.is_none() && inside(state, anchor);
        keep.push(hit == Some(popup) || nested || on_anchor);
    }
    for ((popup, ..), keep) in shown.into_iter().zip(keep) {
        if !keep {
            hide_popup(state, popup)?;
        }
    }
    Ok(())
}

/// Escape hides the topmost popup; Up/Down cycle focus inside it.
/// Returns whether the key was consumed.
pub(crate) fn popup_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    let Some(popup) = state
        .ext_ref::<Popups>()
        .and_then(|p| p.entries.iter().rev().find(|e| e.shown))
        .map(|e| e.popup)
    else {
        return Ok(false);
    };
    if key.key == Key::Escape {
        hide_popup(state, popup)?;
        return Ok(true);
    }
    let forward = match key.key {
        Key::Down => true,
        Key::Up => false,
        _ => return Ok(false),
    };
    let controls = controls(state, popup);
    if controls.is_empty() {
        return Ok(false);
    }
    let current = state.focus.current(&state.tree);
    let index = controls.iter().position(|&n| Some(n) == current);
    let next = match (index, forward) {
        (Some(i), true) => (i + 1) % controls.len(),
        (Some(i), false) => (i + controls.len() - 1) % controls.len(),
        (None, true) => 0,
        (None, false) => controls.len() - 1,
    };
    state.set_focus(Some(controls[next]))?;
    Ok(true)
}

/// Forgets data of removed nodes.
pub(crate) fn removed(state: &mut State, node: NodeId) {
    if state.ext_ref::<Popups>().is_some() {
        let popups = popups(state);
        popups.dropdowns.remove(&node);
        popups.bars.retain(|&bar| bar != node);
    }
}

/// Removes popups whose anchors no longer exist.
pub(crate) fn prune_popups(state: &mut State) -> Result {
    loop {
        let Some(entries) = state.ext_ref::<Popups>().map(|p| &p.entries) else {
            return Ok(());
        };
        let Some(index) = entries
            .iter()
            .position(|e| state.tree.get(e.anchor).is_none() || state.tree.get(e.popup).is_none())
        else {
            return Ok(());
        };
        let entry = popups(state).entries.remove(index);
        if state.tree.get(entry.popup).is_some() {
            state.remove_subtree(entry.popup)?;
        }
    }
}
