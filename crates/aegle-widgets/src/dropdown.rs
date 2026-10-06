//! Dropdown: a button that opens a popup list of choices.

use std::{cell::RefCell, rc::Rc};

use aegle_core::{Dirty, NodeId};
use aegle_ui::{Container, Node, Result, UiError, handle};

use crate::{
    NodePopup, Popup, Widgets,
    button::Variant,
    group::{Group, Role},
    popup::{hide_popup, popups},
};

type ChangeHandler = Rc<RefCell<Option<Box<dyn FnMut(Dropdown) -> Result>>>>;

pub(crate) struct DropdownData {
    pub items: Vec<String>,
    pub selected: usize,
    pub popup: NodeId,
    pub handler: ChangeHandler,
}

handle!(
    Dropdown,
    "A button showing the selected choice; activating it opens a popup list of all choices."
);

pub(crate) fn dropdown(container: &Container, items: &[&str], selected: usize) -> Result<Dropdown> {
    if selected >= items.len() {
        return Err(UiError::InvalidValue.into());
    }
    let button = container.button(items[selected])?;
    let popup = button.popup()?;
    let handler: ChangeHandler = Rc::new(RefCell::new(None));
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
            handler,
        };
        popups(state).dropdowns.insert(id, data);
        Ok(())
    })?;
    let dropdown = Dropdown(button.0.clone());
    dropdown.options(&popup, items, selected)?;
    let opened = dropdown.clone();
    button.on_click(move |_| {
        if popup.is_shown()? {
            return popup.hide();
        }
        popup.show()?;
        // Opening starts at the current choice.
        opened.change(|state, id| {
            let data = &popups(state).dropdowns[&id];
            let (popup, selected) = (data.popup, data.selected);
            let option = state.tree.child(popup, selected)?;
            state.set_focus(option)
        })
    })?;
    Ok(dropdown)
}

impl Dropdown {
    fn options(&self, popup: &Popup, items: &[&str], selected: usize) -> Result {
        for (index, &item) in items.iter().enumerate() {
            let option = popup.button(item)?;
            option.set_border_width(0.0)?;
            option.change(|state, id| {
                if let Some(control) = state.control_as::<crate::button::ButtonControl>(id) {
                    control.variant = Variant::Option {
                        chosen: index == selected,
                    };
                }
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
            let data = popups(state).dropdowns.get_mut(&id).unwrap();
            let changed = data.selected != index;
            data.selected = index;
            let (popup, result) = (
                data.popup,
                (data.items[index].clone(), data.handler.clone(), changed),
            );
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
        self.change(|state, id| Ok(popups(state).dropdowns[&id].selected))
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
        self.select(selected, false)
    }
    /// Replaces the handler run when the user chooses a different item. It runs
    /// outside UI borrows; programmatic selection does not invoke it.
    pub fn on_change(&self, callback: impl FnMut(Dropdown) -> Result + 'static) -> Result {
        let handler = self.change(|state, id| Ok(popups(state).dropdowns[&id].handler.clone()))?;
        *handler.borrow_mut() = Some(Box::new(callback));
        Ok(())
    }
    /// Removes the change handler.
    pub fn clear_on_change(&self) -> Result {
        let handler = self.change(|state, id| Ok(popups(state).dropdowns[&id].handler.clone()))?;
        handler.borrow_mut().take();
        Ok(())
    }
}
