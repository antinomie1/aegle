//! A tab list over pages, one page shown at a time.

use aegle_controls::{Input, Key, KeyInput};
use aegle_core::{Dirty, NodeId};
use aegle_theme::ControlKind;
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
}

impl Control for TabsControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
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

fn data(state: &mut State, root: NodeId) -> &mut TabsControl {
    state.control_as::<TabsControl>(root).expect("a tabs root")
}

/// Activating a tab, run as its button's deferred work: shows its page and,
/// when the selection changed, queues the tab list's change handlers.
pub(crate) fn chosen(state: &mut State, tab: NodeId) -> Result {
    let Some(root) = state
        .tree
        .parent(tab)?
        .and_then(|bar| state.tree.parent(bar).ok()?)
    else {
        return Ok(());
    };
    let index = data(state, root).entries.iter().position(|e| e.0 == tab);
    if let Some(index) = index
        && show(state, root, index)?
    {
        state.queue_action(root);
    }
    Ok(())
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
        let selected = Variant::Tab { selected: false };
        let tab = crate::button::create_as(&bar, title, selected)?;
        let page = group::add(&self.0, Role::TabPanel, false)?;
        page.set_grow(1.0)?;
        page.set_min_width(0.0)?;
        page.set_min_height(0.0)?;
        self.change(|state, id| {
            let tabs = data(state, id);
            tabs.entries.push((tab.id, page.id));
            let selected = tabs.selected.min(tabs.entries.len() - 1);
            show(state, id, selected).map(drop)
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
    /// Adds a handler called after the user selects another tab; handlers
    /// run in registration order.
    pub fn on_change(&self, mut callback: impl FnMut(Tabs) -> Result + 'static) -> Result {
        self.change(|state, id| state.on_action(id, move |node| callback(Tabs(Container(node)))))
    }
}

/// Left/Right on a focused tab focus and select its neighbor, wrapping; the
/// next tab is to the left right to left.
pub(crate) fn tab_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    let right = match key.key {
        Key::Right => true,
        Key::Left => false,
        _ => return Ok(false),
    };
    let Some(id) = state.focus.current(&state.tree) else {
        return Ok(false);
    };
    let forward = right != state.rtl(id);
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
            }) as Box<dyn Control>,
            aegle_ui::container_style(theme, false),
        ))
    })?;
    let root = Container(root);
    let bar = group::add(&root, Role::TabList, true)?;
    bar.set_gap(0.0, 0.0)?;
    root.change(|state, id| {
        data(state, id).bar = Some(bar.id);
        Ok(())
    })?;
    Ok(Tabs(root))
}
