//! In-window popups and the dropdown built on them.

use std::{cell::RefCell, ops::Deref, rc::Rc};

use aegle_controls::{Key, KeyInput};
use aegle_core::{Dirty, NodeId};
use aegle_layout::{Edges, LengthPercentage, LengthPercentageAuto, Position};
use aegle_types::{Point, Rect};

use crate::{
    Container, Node, Result, Style, UiError,
    handles::handle,
    state::{Content, Semantic, State},
    ui::container_style,
};

pub(crate) struct PopupEntry {
    pub popup: NodeId,
    pub anchor: NodeId,
    pub shown: bool,
    /// Focus to restore when the popup closes with focus inside it.
    pub restore: Option<NodeId>,
}

type ChangeHandler = Rc<RefCell<Option<Box<dyn FnMut(Dropdown) -> Result>>>>;

pub(crate) struct DropdownData {
    pub items: Vec<String>,
    pub selected: usize,
    pub popup: NodeId,
    handler: ChangeHandler,
}

/// A floating column drawn above every other control of its window, below its
/// anchor control, or above it when only that fits. It does not take layout
/// space. A press outside it and its anchor, or Escape, hides it; Up and Down
/// move focus among its controls. Removing the anchor removes the popup.
#[derive(Clone)]
pub struct Popup(Container);

impl Deref for Popup {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.0
    }
}

impl Node {
    /// Creates a hidden, empty popup anchored to this control. Add content
    /// through the popup's container methods, then [`Popup::show`] it. It uses
    /// the anchor's theme and the theme surface with a border.
    pub fn popup(&self) -> Result<Popup> {
        let id = self.change(|state, anchor| {
            let theme = *state.theme_of(anchor);
            let mut style = container_style(&theme, false);
            style.position = Position::Absolute;
            style.inset = Edges {
                left: LengthPercentageAuto::length(0.0),
                top: LengthPercentageAuto::length(0.0),
                right: LengthPercentageAuto::auto(),
                bottom: LengthPercentageAuto::auto(),
            };
            let padding = LengthPercentage::length(theme.padding / 2.0);
            style.padding = Edges {
                left: padding,
                right: padding,
                top: padding,
                bottom: padding,
            };
            style.gap = aegle_layout::Size {
                width: LengthPercentage::length(0.0),
                height: LengthPercentage::length(0.0),
            };
            let root = state.root;
            let id = state.insert(root, usize::MAX, Content::Container, style)?;
            state.tree.get_mut(id).unwrap().context.semantic = Semantic::Popup;
            state.set_visible(id, false)?;
            let anchor_element = &state.tree.get(anchor).unwrap().context;
            if anchor_element.theme.is_some() {
                let local = anchor_element.theme.clone();
                state.propagate_theme(id, local)?;
            }
            state.set_style(
                id,
                Style {
                    background: Some(theme.surface),
                    border_color: Some(theme.border),
                    border_width: Some(1.0),
                    ..Default::default()
                },
            )?;
            state.popups.push(PopupEntry {
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
        self.change(|state, id| state.show_popup(id))
    }
    /// Hides the popup, returning focus to where it was when shown.
    pub fn hide(&self) -> Result {
        self.change(|state, id| state.hide_popup(id))
    }
    /// Whether the popup is currently shown.
    pub fn is_shown(&self) -> Result<bool> {
        self.change(|state, id| Ok(state.popups.iter().any(|e| e.popup == id && e.shown)))
    }
}

impl State {
    fn entry(&mut self, popup: NodeId) -> &mut PopupEntry {
        self.popups.iter_mut().find(|e| e.popup == popup).unwrap()
    }

    pub fn show_popup(&mut self, id: NodeId) -> Result {
        if self.entry(id).shown {
            return Ok(());
        }
        // The root's last child paints over, and is hit before, everything else.
        let root = self.root;
        self.tree.reparent(id, Some(root))?;
        self.invalidate_structure();
        let anchor = self.entry(id).anchor;
        let width = self.tree.get(anchor).unwrap().context.bounds.size.width;
        let mut style = self.tree.get(id).unwrap().style().clone();
        style.min_size.width = LengthPercentageAuto::length(width);
        aegle_layout::set_style(&mut self.tree, id, style)?;
        self.set_visible(id, true)?;
        let restore = self.focus.current(&self.tree);
        let entry = self.entry(id);
        entry.shown = true;
        entry.restore = restore;
        self.tree.mark_dirty(anchor, Dirty::SEMANTICS)?;
        self.rebuild_order();
        let first = self.order.iter().copied().find(|&n| {
            n != id
                && self.contains(id, n)
                && self.tree.get(n).unwrap().context.content.interactive()
        });
        if let Some(first) = first {
            self.set_focus(Some(first))?;
        }
        Ok(())
    }

    pub fn hide_popup(&mut self, id: NodeId) -> Result {
        let entry = self.entry(id);
        if !entry.shown {
            return Ok(());
        }
        entry.shown = false;
        let (anchor, restore) = (entry.anchor, entry.restore.take());
        let inside = self
            .focus
            .current(&self.tree)
            .is_some_and(|f| self.contains(id, f));
        self.set_visible(id, false)?;
        if inside && restore.is_some_and(|r| self.usable(r)) {
            self.set_focus(restore)?;
        }
        self.tree.mark_dirty(anchor, Dirty::SEMANTICS)?;
        Ok(())
    }

    /// Places shown popups by their anchors after geometry; returns whether any moved.
    pub fn place_popups(&mut self) -> bool {
        let mut moved = false;
        for index in 0..self.popups.len() {
            let PopupEntry {
                popup,
                anchor,
                shown,
                ..
            } = self.popups[index];
            if !shown {
                continue;
            }
            let anchor = self.tree.get(anchor).unwrap().context.bounds;
            let size = self.tree.get(popup).unwrap().bounds().size;
            let x = anchor.origin.x.min(self.size.width - size.width).max(0.0);
            let below = anchor.origin.y + anchor.size.height;
            let above = anchor.origin.y - size.height;
            let y = if below + size.height <= self.size.height || above < 0.0 {
                below
            } else {
                above
            };
            let element = &mut self.tree.get_mut(popup).unwrap().context;
            if element.offset != Point::new(x, y) {
                element.offset = Point::new(x, y);
                moved = true;
            }
        }
        moved
    }

    /// The topmost shown popup containing a window point, if any.
    pub fn popup_at(&self, position: Point) -> Option<NodeId> {
        let shown = self.popups.iter().rev().filter(|e| e.shown);
        shown
            .map(|e| e.popup)
            .find(|&p| self.tree.get(p).unwrap().context.bounds.contains(position))
    }

    /// Hides shown popups unless a press lands in them, their anchor, or a
    /// popup anchored inside them.
    pub fn dismiss_popups(&mut self, position: Point) -> Result {
        let inside = |state: &State, node: NodeId| {
            let bounds: Rect = state.tree.get(node).unwrap().context.bounds;
            bounds.contains(position)
        };
        let hit = self.popup_at(position);
        let mut keep = Vec::new();
        for entry in self.popups.iter().filter(|e| e.shown) {
            let nested = hit.is_some_and(|hit| {
                let anchor = self.popups.iter().find(|e| e.popup == hit).unwrap().anchor;
                self.contains(entry.popup, anchor)
            });
            keep.push(hit == Some(entry.popup) || nested || inside(self, entry.anchor));
        }
        let shown: Vec<_> = self
            .popups
            .iter()
            .filter(|e| e.shown)
            .map(|e| e.popup)
            .collect();
        for (popup, keep) in shown.into_iter().zip(keep) {
            if !keep {
                self.hide_popup(popup)?;
            }
        }
        Ok(())
    }

    /// Escape hides the topmost popup; Up/Down cycle focus inside it.
    /// Returns whether the key was consumed.
    pub fn popup_key(&mut self, key: &KeyInput<'_>) -> Result<bool> {
        let Some(popup) = self.popups.iter().rev().find(|e| e.shown).map(|e| e.popup) else {
            return Ok(false);
        };
        if key.key == Key::Escape {
            self.hide_popup(popup)?;
            return Ok(true);
        }
        let forward = match key.key {
            Key::Down => true,
            Key::Up => false,
            _ => return Ok(false),
        };
        self.rebuild_order();
        let controls: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|&n| {
                self.contains(popup, n)
                    && self.tree.get(n).unwrap().context.content.interactive()
                    && self.usable(n)
            })
            .collect();
        if controls.is_empty() {
            return Ok(false);
        }
        let current = self.focus.current(&self.tree);
        let index = controls.iter().position(|&n| Some(n) == current);
        let next = match (index, forward) {
            (Some(i), true) => (i + 1) % controls.len(),
            (Some(i), false) => (i + controls.len() - 1) % controls.len(),
            (None, true) => 0,
            (None, false) => controls.len() - 1,
        };
        self.set_focus(Some(controls[next]))?;
        Ok(true)
    }

    /// Removes popups whose anchors no longer exist.
    pub fn prune_popups(&mut self) -> Result {
        while let Some(index) = self
            .popups
            .iter()
            .position(|e| self.tree.get(e.anchor).is_none() || self.tree.get(e.popup).is_none())
        {
            let entry = self.popups.remove(index);
            if self.tree.get(entry.popup).is_some() {
                self.remove_subtree(entry.popup)?;
            }
        }
        Ok(())
    }
}

handle!(
    Dropdown,
    "A button showing the selected choice; activating it opens a popup list of all choices."
);

impl Container {
    /// Appends a dropdown with at least one choice and a valid selected index.
    /// The choice list opens below it; Up/Down and Enter or a click choose.
    pub fn dropdown(&self, items: &[&str], selected: usize) -> Result<Dropdown> {
        if selected >= items.len() {
            return Err(UiError::InvalidValue.into());
        }
        let button = self.button(items[selected])?;
        let popup = button.popup()?;
        let handler: ChangeHandler = Rc::new(RefCell::new(None));
        button.change(|state, id| {
            state.tree.get_mut(id).unwrap().context.semantic = Semantic::Dropdown;
            state.tree.mark_dirty(id, Dirty::ALL)?;
            let items = items.iter().map(|&item| item.to_owned()).collect();
            let data = DropdownData {
                items,
                selected,
                popup: popup.id,
                handler,
            };
            state.dropdowns.insert(id, data);
            Ok(())
        })?;
        let dropdown = Dropdown(button.0.clone());
        dropdown.options(&popup, items)?;
        let opened = dropdown.clone();
        button.on_click(move |_| {
            if popup.is_shown()? {
                return popup.hide();
            }
            popup.show()?;
            // Opening starts at the current choice.
            opened.change(|state, id| {
                let data = &state.dropdowns[&id];
                let option = state.tree.child(data.popup, data.selected)?;
                state.set_focus(option)
            })
        })?;
        Ok(dropdown)
    }
}

impl Dropdown {
    fn options(&self, popup: &Popup, items: &[&str]) -> Result {
        for (index, &item) in items.iter().enumerate() {
            let option = popup.button(item)?;
            option.set_border_width(0.0)?;
            option.change(|state, id| {
                state.tree.get_mut(id).unwrap().context.semantic = Semantic::Option;
                state.tree.mark_dirty(id, Dirty::ALL)?;
                Ok(())
            })?;
            let dropdown = self.clone();
            option.on_click(move |_| dropdown.select(index, true))?;
        }
        Ok(())
    }

    fn select(&self, index: usize, notify: bool) -> Result {
        let (text, handler, changed) = self.change(|state, id| {
            let data = state.dropdowns.get_mut(&id).unwrap();
            let changed = data.selected != index;
            data.selected = index;
            let (popup, result) = (
                data.popup,
                (data.items[index].clone(), data.handler.clone(), changed),
            );
            // Options draw and export their selected state.
            for option in state.tree.children(popup)?.collect::<Vec<_>>() {
                state
                    .tree
                    .mark_dirty(option, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            state.hide_popup(popup)?;
            Ok(result)
        })?;
        self.0.set_text(&text)?;
        if notify && changed {
            let callback = handler.borrow_mut().take();
            if let Some(mut callback) = callback {
                let result = callback(self.clone());
                let mut slot = handler.borrow_mut();
                if slot.is_none() {
                    *slot = Some(callback);
                }
                result?;
            }
        }
        Ok(())
    }

    /// The selected choice index.
    pub fn selected(&self) -> Result<usize> {
        self.change(|state, id| Ok(state.dropdowns[&id].selected))
    }
    /// Selects a valid choice without invoking the change handler.
    pub fn set_selected(&self, index: usize) -> Result {
        if index >= self.items()?.len() {
            return Err(UiError::InvalidValue.into());
        }
        self.select(index, false)
    }
    /// Copies the choices.
    pub fn items(&self) -> Result<Vec<String>> {
        self.change(|state, id| Ok(state.dropdowns[&id].items.clone()))
    }
    /// Replaces the choices and selection without invoking the change handler.
    pub fn set_items(&self, items: &[&str], selected: usize) -> Result {
        if selected >= items.len() {
            return Err(UiError::InvalidValue.into());
        }
        let popup = self.change(|state, id| {
            let popup = state.dropdowns[&id].popup;
            state.hide_popup(popup)?;
            for option in state.tree.children(popup)?.collect::<Vec<_>>() {
                state.remove_subtree(option)?;
            }
            let data = state.dropdowns.get_mut(&id).unwrap();
            data.items = items.iter().map(|&item| item.to_owned()).collect();
            Ok(popup)
        })?;
        let popup = Popup(Container(Node {
            state: self.state.clone(),
            id: popup,
        }));
        self.options(&popup, items)?;
        self.select(selected, false)
    }
    /// Replaces the handler run when the user chooses a different item. It runs
    /// outside UI borrows; programmatic selection does not invoke it.
    pub fn on_change(&self, callback: impl FnMut(Dropdown) -> Result + 'static) -> Result {
        let handler = self.change(|state, id| Ok(state.dropdowns[&id].handler.clone()))?;
        *handler.borrow_mut() = Some(Box::new(callback));
        Ok(())
    }
    /// Removes the change handler.
    pub fn clear_on_change(&self) -> Result {
        let handler = self.change(|state, id| Ok(state.dropdowns[&id].handler.clone()))?;
        handler.borrow_mut().take();
        Ok(())
    }
}

impl State {
    /// Whether a popup is a dropdown's choice list.
    #[cfg(feature = "accessibility")]
    pub fn popup_lists(&self, popup: NodeId) -> bool {
        let entry = self.popups.iter().find(|e| e.popup == popup);
        entry.is_some_and(|e| self.dropdowns.contains_key(&e.anchor))
    }

    /// For a choice inside a dropdown popup: whether it is the selected one.
    pub fn option_selected(&self, option: NodeId) -> Option<bool> {
        let popup = self.tree.parent(option).ok().flatten()?;
        let entry = self.popups.iter().find(|e| e.popup == popup)?;
        let data = self.dropdowns.get(&entry.anchor)?;
        let index = self.tree.children(popup).ok()?.position(|n| n == option)?;
        Some(index == data.selected)
    }

    /// For a dropdown anchor: whether its choice list is shown.
    #[cfg(feature = "accessibility")]
    pub fn dropdown_expanded(&self, anchor: NodeId) -> bool {
        let popup = self.dropdowns.get(&anchor).map(|d| d.popup);
        self.popups
            .iter()
            .any(|e| Some(e.popup) == popup && e.shown)
    }
}
