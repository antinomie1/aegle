//! Dropdown: a button that opens a popup list of choices.

use aegle_core::{Dirty, NodeId};
use aegle_ui::{Container, Node, Result, State, UiError, handle};

use crate::{
    NodePopup, Popup, Widgets,
    button::Variant,
    group::{Group, Role},
    popup::{entry, hide_popup, popups, show_popup},
};

pub(crate) struct DropdownData {
    pub items: Vec<String>,
    pub selected: usize,
    pub popup: NodeId,
}

handle! {
    /// A button showing the selected choice; activating it opens a popup list of all choices.
    pub Dropdown(crate::button::ButtonControl): text, interactive, pressed
}

pub(crate) fn dropdown(container: &Container, items: &[&str], selected: usize) -> Result<Dropdown> {
    if selected >= items.len() {
        return Err(UiError::InvalidValue.into());
    }
    let button = container.button(items[selected])?;
    let popup = button.popup()?;
    button.change(|state, id| {
        if let Some(control) = state.control_as::<crate::button::ButtonControl>(id) {
            control.variant = Variant::Dropdown { expanded: false };
        }
        if let Some(group) = state.control_as::<Group>(popup.id) {
            group.role = Role::Popup { list: true };
        }
        state.tree.mark_dirty(id, Dirty::ALL)?;
        let items = items.iter().map(|&item| item.to_owned()).collect();
        let data = DropdownData {
            items,
            selected,
            popup: popup.id,
        };
        popups(state).dropdowns.insert(id, data);
        Ok(())
    })?;
    let dropdown = Dropdown(button.0.clone());
    dropdown.options(&popup, items, selected)?;
    Ok(dropdown)
}

/// Activating a dropdown: closes a shown choice list, else opens it focused
/// at the current choice. It is the button's own behavior, so the dropdown's
/// handlers hear only choices.
pub(crate) fn toggle(state: &mut State, id: NodeId) -> Result {
    let data = &popups(state).dropdowns[&id];
    let (popup, selected) = (data.popup, data.selected);
    if entry(state, popup).shown {
        return hide_popup(state, popup);
    }
    entry(state, popup).at = None;
    show_popup(state, popup, true)?;
    let option = state.tree.child(popup, selected)?;
    state.set_focus(option)
}

/// Choosing an option, run as its button's deferred work.
pub(crate) fn chosen(state: &mut State, option: NodeId) -> Result {
    let Some(popup) = state.tree.parent(option)? else {
        return Ok(());
    };
    let dropdown = popups(state)
        .dropdowns
        .iter()
        .find_map(|(&id, data)| (data.popup == popup).then_some(id));
    let index = state
        .tree
        .children(popup)?
        .position(|child| child == option);
    match (dropdown, index) {
        (Some(id), Some(index)) => select(state, id, index, true),
        _ => Ok(()),
    }
}

/// Selects choice `index`, marks the options, closes the list and, for a
/// user choice that changed the selection, queues the change handlers.
fn select(state: &mut State, id: NodeId, index: usize, user: bool) -> Result {
    let data = popups(state).dropdowns.get_mut(&id).unwrap();
    let changed = data.selected != index;
    data.selected = index;
    let (popup, text) = (data.popup, data.items[index].clone());
    // Options draw and export their selected state.
    for (position, option) in state
        .tree
        .children(popup)?
        .collect::<Vec<_>>()
        .into_iter()
        .enumerate()
    {
        if let Some(control) = state.control_as::<crate::button::ButtonControl>(option) {
            control.variant = Variant::Option {
                chosen: position == index,
            };
        }
        state
            .tree
            .mark_dirty(option, Dirty::PAINT | Dirty::SEMANTICS)?;
    }
    hide_popup(state, popup)?;
    if user && changed {
        state.queue_action(id);
    }
    state.set_text(id, &text)
}

impl Dropdown {
    fn options(&self, popup: &Popup, items: &[&str], selected: usize) -> Result {
        for (index, &item) in items.iter().enumerate() {
            let chosen = index == selected;
            crate::button::create_as(popup, item, Variant::Option { chosen })?;
        }
        Ok(())
    }

    /// The selected choice index.
    pub fn selected(&self) -> Result<usize> {
        self.change(|state, id| Ok(popups(state).dropdowns[&id].selected))
    }
    /// Selects a valid choice without invoking the change handler.
    pub fn set_selected(&self, index: usize) -> Result {
        if index >= self.items()?.len() {
            return Err(UiError::InvalidValue.into());
        }
        self.change(|state, id| select(state, id, index, false))
    }
    /// Copies the choices.
    pub fn items(&self) -> Result<Vec<String>> {
        self.change(|state, id| Ok(popups(state).dropdowns[&id].items.clone()))
    }
    /// Replaces the choices and selection without invoking the change handler.
    pub fn set_items(&self, items: &[&str], selected: usize) -> Result {
        if selected >= items.len() {
            return Err(UiError::InvalidValue.into());
        }
        let popup = self.change(|state, id| {
            let popup = popups(state).dropdowns[&id].popup;
            hide_popup(state, popup)?;
            for option in state.tree.children(popup)?.collect::<Vec<_>>() {
                state.remove_subtree(option)?;
            }
            let data = popups(state).dropdowns.get_mut(&id).unwrap();
            data.items = items.iter().map(|&item| item.to_owned()).collect();
            Ok(popup)
        })?;
        let popup = Popup(Container(Node {
            state: self.state.clone(),
            id: popup,
        }));
        self.options(&popup, items, selected)?;
        self.change(|state, id| select(state, id, selected, false))
    }
    /// Adds a handler run when the user chooses a different item. Handlers
    /// run in registration order outside UI borrows; programmatic selection does not invoke it.
    pub fn on_change(&self, mut callback: impl FnMut(Dropdown) -> Result + 'static) -> Result {
        self.change(|state, id| state.on_action(id, move |node| callback(Dropdown(node))))
    }
}
