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
}

/// Popups and dropdowns of one UI, kept in the engine's per-library storage.
#[derive(Default)]
pub(crate) struct Popups {
    /// Shown or hidden popups with their anchors, in showing order.
    pub entries: Vec<PopupEntry>,
    /// Dropdown anchors and their choices.
    pub dropdowns: HashMap<NodeId, DropdownData>,
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

/// Creates popups anchored to a control.
pub trait NodePopup {
    /// Creates a hidden, empty popup anchored to this control. Add content
    /// through the popup's container methods, then [`Popup::show`] it. It uses
    /// the anchor's theme and the theme surface with a border.
    fn popup(&self) -> Result<Popup>;
}

impl NodePopup for Node {
    fn popup(&self) -> Result<Popup> {
        let id = self.change(|state, anchor| {
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
            group.retheme(&theme, 0, false, &mut style);
            let id = state.insert(root, usize::MAX, Box::new(group), style)?;
            state.set_visible(id, false)?;
            let anchor_element = &state.tree.get(anchor).unwrap().context;
            if anchor_element.theme.is_some() {
                let local = anchor_element.theme.clone();
                state.propagate_theme(id, local)?;
            }
            state.decorations.entry(id).or_default().skin = Some(crate::group::panel);
            popups(state).entries.push(PopupEntry {
                popup: id,
                anchor,
                shown: false,
                restore: None,
            });
            Ok(id)
        })?;
        Ok(Popup(Container(Node {
            state: self.state.clone(),
            id,
        })))
    }
}

impl Popup {
    /// Shows the popup above all controls, at least as wide as its anchor,
    /// and focuses its first enabled control.
    pub fn show(&self) -> Result {
        self.change(show_popup)
    }
    /// Hides the popup, returning focus to where it was when shown.
    pub fn hide(&self) -> Result {
        self.change(hide_popup)
    }
    /// Whether the popup is currently shown.
    pub fn is_shown(&self) -> Result<bool> {
        self.change(|state, id| {
            Ok(popups(state)
                .entries
                .iter()
                .any(|e| e.popup == id && e.shown))
        })
    }
}

fn entry(state: &mut State, popup: NodeId) -> &mut PopupEntry {
    popups(state)
        .entries
        .iter_mut()
        .find(|e| e.popup == popup)
        .unwrap()
}

fn set_expanded(state: &mut State, anchor: NodeId, expanded: bool) {
    if let Some(button) = state.control_as::<crate::button::ButtonControl>(anchor) {
        if let Variant::Dropdown { expanded: old } = &mut button.variant {
            *old = expanded;
        }
    }
}

pub(crate) fn show_popup(state: &mut State, id: NodeId) -> Result {
    if entry(state, id).shown {
        return Ok(());
    }
    // The root's last child paints over, and is hit before, everything else.
    let root = state.root;
    state.tree.reparent(id, Some(root))?;
    state.invalidate_structure();
    let anchor = entry(state, id).anchor;
    let width = state.tree.get(anchor).unwrap().context.bounds.size.width;
    let mut style = state.tree.get(id).unwrap().style().clone();
    style.min_size.width = LengthPercentageAuto::length(width);
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
    let first = state.order.iter().copied().find(|&n| {
        n != id && state.contains(id, n) && state.tree.get(n).unwrap().context.control.interactive()
    });
    if let Some(first) = first {
        state.set_focus(Some(first))?;
    }
    Ok(())
}

pub(crate) fn hide_popup(state: &mut State, id: NodeId) -> Result {
    let e = entry(state, id);
    if !e.shown {
        return Ok(());
    }
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
            .map(|e| (e.popup, e.anchor))
            .collect(),
        None => return false,
    };
    for (popup, anchor) in shown {
        let rtl = state.rtl(anchor);
        let anchor = state.tree.get(anchor).unwrap().context.bounds;
        let size = state.tree.get(popup).unwrap().bounds().size;
        // Aligned with the anchor's start edge: its right edge right to left.
        let start = if rtl {
            anchor.origin.x + anchor.size.width - size.width
        } else {
            anchor.origin.x
        };
        let x = start.min(state.size.width - size.width).max(0.0);
        let below = anchor.origin.y + anchor.size.height;
        let above = anchor.origin.y - size.height;
        let y = if below + size.height <= state.size.height || above < 0.0 {
            below
        } else {
            above
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
            .map(|e| (e.popup, e.anchor))
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
    for &(popup, anchor) in &shown {
        let nested = hit.is_some_and(|hit| {
            let hit_anchor = state
                .ext_ref::<Popups>()
                .and_then(|p| p.entries.iter().find(|e| e.popup == hit))
                .unwrap()
                .anchor;
            state.contains(popup, hit_anchor)
        });
        keep.push(hit == Some(popup) || nested || inside(state, anchor));
    }
    for ((popup, _), keep) in shown.into_iter().zip(keep) {
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
    state.rebuild_order();
    let controls: Vec<_> = state
        .order
        .iter()
        .copied()
        .filter(|&n| {
            state.contains(popup, n)
                && state.tree.get(n).unwrap().context.control.interactive()
                && state.usable(n)
        })
        .collect();
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
        popups(state).dropdowns.remove(&node);
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
