use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use aegle_core::{Dirty, NodeId};
use aegle_layout::{FlexDirection, Style};
use aegle_types::Rect;

use crate::{
    Result, Theme, UiError,
    control::{Control, Plain},
    state::State,
    ui::container_style,
};

/// Weak generational identity. Cloning a handle neither copies nor owns its control.
#[derive(Clone)]
pub struct Node {
    /// The owning UI's state.
    pub state: Weak<RefCell<State>>,
    /// The node's identity in that state's tree.
    pub id: NodeId,
}

impl Node {
    /// Runs `change` on the live node with the UI borrowed; the entry point for
    /// control libraries' typed handles.
    pub fn change<T>(&self, change: impl FnOnce(&mut State, NodeId) -> Result<T>) -> Result<T> {
        let owner = self.state.upgrade().ok_or(UiError::DeadHandle)?;
        let mut state = owner
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if state.tree.get(self.id).is_none() {
            return Err(UiError::DeadHandle.into());
        }
        change(&mut state, self.id)
    }
    /// Whether this node and its owning UI still exist.
    pub fn is_alive(&self) -> bool {
        self.state
            .upgrade()
            .is_some_and(|s| s.borrow().tree.get(self.id).is_some())
    }
    /// Last refreshed geometry in logical window coordinates.
    pub fn bounds(&self) -> Result<Rect> {
        self.change(|state, id| Ok(state.tree.get(id).unwrap().context.bounds))
    }
    /// Removes this control and every descendant, cancelling focus, capture and callbacks.
    pub fn remove(&self) -> Result {
        self.change(|state, id| {
            if id == state.root {
                return Err(UiError::RootMutation.into());
            }
            state.remove_subtree(id)
        })
    }
    /// Moves this subtree to the end of another container in the same UI.
    pub fn reparent(&self, parent: &Container) -> Result {
        if !Weak::ptr_eq(&self.state, &parent.state) {
            return Err(UiError::ForeignUi.into());
        }
        self.change(|state, id| {
            if id == state.root {
                return Err(UiError::RootMutation.into());
            }
            if state.tree.get(parent.id).is_none() {
                return Err(UiError::DeadHandle.into());
            }
            state.tree.reparent(id, Some(parent.id))?;
            #[cfg(feature = "grid")]
            {
                let mut style = state.tree.get(id).unwrap().style().clone();
                crate::grid_handles::stack_child(state, parent.id, &mut style);
                aegle_layout::set_style(&mut state.tree, id, style)?;
            }
            if !state.usable(id) {
                state.cancel_subtree(id)?;
            }
            state.invalidate_structure();
            let element = &state.tree.get(id).unwrap().context;
            let local = element.theme.clone().filter(|_| element.local_theme);
            state.propagate_theme(id, local)
        })
    }
    /// Shows or hides the entire subtree. Hidden controls take no layout space.
    pub fn set_visible(&self, visible: bool) -> Result {
        self.change(|state, id| state.set_visible(id, visible))
    }
    /// Disables interaction throughout this subtree, preserving its displayed values.
    pub fn set_enabled(&self, enabled: bool) -> Result {
        self.change(|state, id| {
            if !enabled {
                state.cancel_subtree(id)?;
            }
            state.tree.get_mut(id).unwrap().context.enabled = enabled;
            let fonts = Rc::clone(&state.fonts);
            let outcome = state
                .tree
                .get_mut(id)
                .unwrap()
                .context
                .control
                .set_enabled(&mut fonts.borrow_mut(), enabled);
            state.effects(id, outcome)?;
            state.rebuild_order();
            for index in 0..state.order.len() {
                let child = state.order[index];
                if state.contains(id, child) {
                    state
                        .tree
                        .mark_dirty(child, Dirty::PAINT | Dirty::SEMANTICS)?;
                }
            }
            Ok(())
        })
    }
    /// Requests logical focus using the same enabled/visible policy as keyboard traversal.
    pub fn focus(&self) -> Result {
        self.change(|state, id| state.set_focus(Some(id)))
    }
    /// Explicit semantic name, including a text field's accessible label.
    pub fn set_accessible_label(&self, label: &str) -> Result {
        self.change(|state, id| {
            state.tree.update(id, Dirty::SEMANTICS, |node| {
                node.context.label.clear();
                node.context.label.push_str(label);
            })?;
            Ok(())
        })
    }
}

/// Defines a typed handle: a clonable wrapper around a [`Node`] that dereferences to it.
#[macro_export]
macro_rules! handle {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone)]
        pub struct $name(pub $crate::Node);
        impl ::std::ops::Deref for $name {
            type Target = $crate::Node;
            fn deref(&self) -> &$crate::Node {
                &self.0
            }
        }
    };
}
handle!(
    Container,
    "A retained row or column. Creation methods append children once."
);

impl Container {
    /// Appends a node whose control and layout style `create` provides, inheriting
    /// the parent's resolved theme. The way control libraries add their controls.
    pub fn add(
        &self,
        create: impl FnOnce(&mut State, &Theme) -> Result<(Box<dyn Control>, Style)>,
    ) -> Result<Node> {
        self.change(|state, parent| {
            let theme = *state.theme_of(parent);
            let (control, style) = create(state, &theme)?;
            let id = state.insert(parent, usize::MAX, control, style)?;
            Ok(Node {
                state: self.state.clone(),
                id,
            })
        })
    }
    /// Appends a vertical container.
    pub fn column(&self) -> Result<Container> {
        self.add(|_, theme| Ok((Box::new(Plain), container_style(theme, false))))
            .map(Container)
    }
    /// Appends a horizontal container.
    pub fn row(&self) -> Result<Container> {
        self.add(|_, theme| {
            let mut style = container_style(theme, false);
            style.flex_direction = FlexDirection::Row;
            Ok((Box::new(Plain), style))
        })
        .map(Container)
    }
    /// Appends a transparent group: its children take part in this
    /// container's layout (row, column, wrap or grid) as if they were its own
    /// children, so they can be shown, hidden or replaced as a unit. Its own
    /// layout settings are ignored; hiding it hides its children.
    pub fn contents(&self) -> Result<Container> {
        let group = self.add(|_, _| Ok((Box::new(Plain), Style::default())))?;
        group.change(|state, id| Ok(aegle_layout::set_contents(&mut state.tree, id, true)?))?;
        Ok(Container(group))
    }
}

/// Rejects nonfinite or negative lengths.
pub fn valid(value: f32) -> Result {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(UiError::InvalidValue.into())
    }
}
