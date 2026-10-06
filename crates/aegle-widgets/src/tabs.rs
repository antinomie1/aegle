//! A tab list over pages, one page shown at a time.

use std::any::Any;

use aegle_controls::{Input, Key, KeyInput};
use aegle_core::{Dirty, NodeId};
use aegle_theme::{Appearance, ControlKind, Theme, VisualState};
use aegle_ui::{Container, Control, Node, Result, State, UiError, control::Frame};

use crate::{
    Button,
    button::{ButtonControl, Variant},
    group::{self, Role},
};

/// The control at a [`Tabs`] root: its tabs, pages and selection.
pub struct TabsControl {
    /// The tab list row, set right after the root is created.
    bar: Option<NodeId>,
    entries: Vec<(NodeId, NodeId)>,
    selected: usize,
    callback: Option<Box<dyn FnMut(Tabs) -> Result>>,
}

impl Control for TabsControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Container
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
}

/// A row of tabs above the selected page. Tabs are buttons that activate
/// with a click, Enter or Space; Left/Right move between them.
#[derive(Clone)]
pub struct Tabs(pub Container);

impl std::ops::Deref for Tabs {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.0
    }
}

/// Tabs draw no box: hover and press tint, the selection is an underline.
fn tab_skin(theme: &Theme, state: VisualState) -> Appearance {
    let base = Appearance::new(theme, state);
    Appearance {
        background: if state.enabled && (state.hovered || state.pressed) {
            base.background
        } else {
            aegle_types::Color::TRANSPARENT
        },
        border_width: 0.0,
        ..base
    }
}

fn data(state: &mut State, root: NodeId) -> &mut TabsControl {
    state.control_as::<TabsControl>(root).expect("a tabs root")
}

/// Shows page `index` and marks its tab; returns whether the selection changed.
fn show(state: &mut State, root: NodeId, index: usize) -> Result<bool> {
    let tabs = data(state, root);
    let previous = tabs.selected;
    tabs.selected = index;
    let entries = tabs.entries.clone();
    for (position, (tab, page)) in entries.into_iter().enumerate() {
        let selected = position == index;
        if let Some(button) = state.control_as::<ButtonControl>(tab) {
            button.variant = Variant::Tab { selected };
        }
        state.set_visible(page, selected)?;
        state
            .tree
            .mark_dirty(tab, Dirty::PAINT | Dirty::SEMANTICS)?;
    }
    Ok(previous != index)
}

impl Tabs {
    /// Adds a tab titled `title` and returns its page, a column. The first
    /// tab added is selected.
    pub fn add(&self, title: &str) -> Result<Container> {
        let bar =
            self.change(|state, id| Ok(data(state, id).bar.expect("created with its tab list")))?;
        let bar = Container(Node {
            state: self.state.clone(),
            id: bar,
        });
        let tab = crate::button::create(&bar, title)?;
        tab.set_skin(tab_skin)?;
        let page = group::add(&self.0, Role::TabPanel, false)?;
        page.set_grow(1.0)?;
        page.set_min_size(0.0, 0.0)?;
        let index = self.change(|state, id| {
            let tabs = data(state, id);
            tabs.entries.push((tab.id, page.id));
            let index = tabs.entries.len() - 1;
            let selected = tabs.selected.min(index);
            show(state, id, selected)?;
            Ok(index)
        })?;
        let tabs = self.clone();
        tab.on_click(move |_| {
            if tabs.change(|state, id| show(state, id, index))? {
                tabs.notify()?;
            }
            Ok(())
        })?;
        Ok(page)
    }
    /// The selected tab's index.
    pub fn selected(&self) -> Result<usize> {
        self.change(|state, id| Ok(data(state, id).selected))
    }
    /// Selects a tab without calling the change handler.
    pub fn select(&self, index: usize) -> Result {
        self.change(|state, id| {
            if index >= data(state, id).entries.len() {
                return Err(UiError::InvalidValue.into());
            }
            show(state, id, index).map(drop)
        })
    }
    /// The tab button at `index`, for its text, style or accessible label.
    pub fn tab(&self, index: usize) -> Result<Button> {
        let tab = self.change(|state, id| {
            data(state, id)
                .entries
                .get(index)
                .map(|entry| entry.0)
                .ok_or(UiError::InvalidValue.into())
        })?;
        Ok(Button(Node {
            state: self.state.clone(),
            id: tab,
        }))
    }
    /// Replaces the handler called after the user selects another tab.
    pub fn on_change(&self, callback: impl FnMut(Tabs) -> Result + 'static) -> Result {
        self.change(|state, id| {
            data(state, id).callback = Some(Box::new(callback));
            Ok(())
        })
    }
    /// Runs the change handler outside the UI borrow.
    fn notify(&self) -> Result {
        let callback = self.change(|state, id| Ok(data(state, id).callback.take()))?;
        if let Some(mut callback) = callback {
            let result = callback(self.clone());
            self.change(|state, id| {
                let slot = &mut data(state, id).callback;
                if slot.is_none() && result.is_ok() {
                    *slot = Some(callback);
                }
                Ok(())
            })?;
            result?;
        }
        Ok(())
    }
}

/// Left/Right on a focused tab focus and select its neighbor, wrapping.
pub(crate) fn tab_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    let forward = match key.key {
        Key::Right => true,
        Key::Left => false,
        _ => return Ok(false),
    };
    let Some(id) = state.focus.current(&state.tree) else {
        return Ok(false);
    };
    let tab = |state: &mut State, node| {
        state
            .control_as::<ButtonControl>(node)
            .is_some_and(|b| matches!(b.variant, Variant::Tab { .. }))
            && state.usable(node)
    };
    let Some(bar) = state.tree.parent(id)?.filter(|_| tab(state, id)) else {
        return Ok(false);
    };
    let children: Vec<_> = state.tree.children(bar)?.collect();
    let row: Vec<_> = children.into_iter().filter(|&n| tab(state, n)).collect();
    let index = row.iter().position(|&n| n == id).unwrap();
    let next = if forward {
        (index + 1) % row.len()
    } else {
        (index + row.len() - 1) % row.len()
    };
    state.set_focus(Some(row[next]))?;
    state.dispatch(row[next], Input::Activate)?;
    Ok(true)
}

pub(crate) fn tabs(container: &Container) -> Result<Tabs> {
    let root = crate::add(container, |_, theme| {
        Ok((
            Box::new(TabsControl {
                bar: None,
                entries: Vec::new(),
                selected: 0,
                callback: None,
            }) as Box<dyn Control>,
            aegle_ui::container_style(theme, false),
        ))
    })?;
    let root = Container(root);
    let bar = group::add(&root, Role::TabList, true)?;
    bar.set_gap(0.0)?;
    root.change(|state, id| {
        data(state, id).bar = Some(bar.id);
        Ok(())
    })?;
    Ok(Tabs(root))
}
