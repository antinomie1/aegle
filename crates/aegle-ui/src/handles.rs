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

/// Panics with a misuse error; see [`Node::change`].
#[cold]
pub(crate) fn fail(error: Box<dyn std::error::Error>) -> ! {
    panic!("{error}")
}

/// Unwraps internal results in handle methods, panicking like
/// [`Node::change`] on an error; for control libraries' handles.
pub trait OrFail<T> {
    /// The value, or a panic with the error.
    fn or_fail(self) -> T;
}

impl<T, E: Into<Box<dyn std::error::Error>>> OrFail<T> for std::result::Result<T, E> {
    fn or_fail(self) -> T {
        self.unwrap_or_else(|error| fail(error.into()))
    }
}

impl Node {
    /// Runs `change` on the live node with the UI borrowed; the entry point for
    /// control libraries' typed handles.
    ///
    /// # Panics
    ///
    /// Misusing a handle is a programming error, so handle methods panic
    /// instead of returning errors: when the control was removed or its
    /// window closed (check [`Self::is_alive`]), when called from a painter,
    /// hook or scene visitor, and when `change` fails, which handle methods
    /// do for the invalid arguments their documentation names.
    pub fn change<T>(&self, change: impl FnOnce(&mut State, NodeId) -> Result<T>) -> T {
        let owner = self.state.upgrade();
        let owner = owner.unwrap_or_else(|| fail(UiError::DeadHandle.into()));
        let mut state = owner
            .try_borrow_mut()
            .unwrap_or_else(|_| fail(UiError::ReentrantAccess.into()));
        if state.tree.get(self.id).is_none() {
            fail(UiError::DeadHandle.into());
        }
        change(&mut state, self.id).unwrap_or_else(|error| fail(error))
    }
    /// Whether this node and its owning UI still exist.
    pub fn is_alive(&self) -> bool {
        self.state.upgrade().is_some_and(|owner| {
            let state = owner
                .try_borrow()
                .unwrap_or_else(|_| fail(UiError::ReentrantAccess.into()));
            state.tree.get(self.id).is_some()
        })
    }
    /// Last refreshed geometry in logical window coordinates.
    pub fn bounds(&self) -> Rect {
        self.change(|state, id| Ok(state.tree.get(id).unwrap().context.bounds))
    }
    /// The first text baseline below the top of [`Self::bounds`] as of the last
    /// layout, the line `Align::Baseline` lines up; `None` without text.
    pub fn baseline(&self) -> Option<f32> {
        self.change(|state, id| {
            let element = &state.tree.get(id).unwrap().context;
            let padding = element.inset(&state.theme);
            Ok(element.control.baseline(element.bounds.size, padding))
        })
    }
    /// Removes this control and every descendant, cancelling focus, capture and callbacks.
    pub fn remove(&self) {
        self.change(|state, id| {
            if id == state.root {
                return Err(UiError::RootMutation.into());
            }
            state.remove_subtree(id)
        })
    }
    /// Moves this subtree to the end of another container in the same UI.
    /// If a bound property rejects its value there, the subtree stays where
    /// it was and this panics.
    pub fn reparent(&self, parent: &Container) {
        if !Weak::ptr_eq(&self.state, &parent.state) {
            panic!("{}", UiError::ForeignUi);
        }
        self.change(|state, id| {
            if id == state.root {
                return Err(UiError::RootMutation.into());
            }
            if state.tree.get(parent.id).is_none() {
                return Err(UiError::DeadHandle.into());
            }
            let old_parent = state.tree.parent(id)?;
            let position = state
                .tree
                .children(old_parent.unwrap())?
                .position(|child| child == id)
                .unwrap();
            let old_style = state.tree.get(id).unwrap().style().clone();
            let element = &state.tree.get(id).unwrap().context;
            let local = element.theme.clone().filter(|_| element.local_theme);
            state.tree.reparent(id, Some(parent.id))?;
            #[cfg(feature = "grid")]
            {
                let mut style = old_style.clone();
                crate::grid_handles::stack_child(state, parent.id, &mut style);
                aegle_layout::set_style(&mut state.tree, id, style)?;
            }
            state.invalidate_structure();
            if let Err(error) = state.propagate_theme(id, local.clone()) {
                state.tree.reparent_at(id, old_parent, position)?;
                aegle_layout::set_style(&mut state.tree, id, old_style)?;
                state.invalidate_structure();
                state.propagate_theme(id, local)?;
                return Err(error);
            }
            if !state.usable(id) {
                state.cancel_subtree(id)?;
            }
            state.resolve_skins(id)?;
            state.propagate_direction(id)
        })
    }
    /// Shows or hides the entire subtree. Hidden controls take no layout space.
    pub fn set_visible(&self, visible: bool) {
        self.change(|state, id| state.set_visible(id, visible))
    }
    /// Disables interaction throughout this subtree, preserving its displayed values.
    pub fn set_enabled(&self, enabled: bool) {
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
    pub fn focus(&self) {
        self.change(|state, id| state.set_focus(Some(id)))
    }
    /// Whether this control has logical focus, whether or not it shows it
    /// (see `VisualState::focused`).
    pub fn is_focused(&self) -> bool {
        self.change(|state, id| Ok(state.focus.current(&state.tree) == Some(id)))
    }
    /// Supplementary text for assistive technology, such as a tooltip's;
    /// `None` removes it.
    pub fn set_accessible_description<'a>(&self, description: impl Into<Option<&'a str>>) {
        let description = description.into();
        self.change(|state, id| {
            match description {
                Some(text) => state.descriptions.insert(id, text.to_owned()),
                None => state.descriptions.remove(&id),
            };
            state.tree.mark_dirty(id, Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Explicit semantic name, including a text field's accessible label.
    pub fn set_accessible_label(&self, label: &str) {
        self.change(|state, id| {
            state.tree.update(id, Dirty::SEMANTICS, |node| {
                node.context.label.clear();
                node.context.label.push_str(label);
            })?;
            Ok(())
        })
    }
}

crate::handle! {
    /// A retained row or column. Creation methods append children once.
    pub Container
}

impl Container {
    /// Appends a node whose control and layout style `create` provides, inheriting
    /// the parent's resolved theme. The way control libraries add their controls.
    pub fn add(
        &self,
        create: impl FnOnce(&mut State, &Theme) -> Result<(Box<dyn Control>, Style)>,
    ) -> Node {
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
    pub fn column(&self) -> Container {
        Container(self.add(|_, theme| Ok((Box::new(Plain), container_style(theme, false)))))
    }
    /// Appends a horizontal container.
    pub fn row(&self) -> Container {
        Container(self.add(|_, theme| {
            let mut style = container_style(theme, false);
            style.flex_direction = FlexDirection::Row;
            Ok((Box::new(Plain), style))
        }))
    }
    /// Appends a transparent group: its children take part in this
    /// container's layout (row, column, wrap or grid) as if they were its own
    /// children, so they can be shown, hidden or replaced as a unit. Its own
    /// layout settings are ignored; hiding it hides its children.
    pub fn contents(&self) -> Container {
        let group = self.add(|_, _| Ok((Box::new(Plain), Style::default())));
        group.change(|state, id| Ok(aegle_layout::set_contents(&mut state.tree, id, true)?));
        Container(group)
    }
    /// Clips the children's painting, hit testing and pointer shapes to this
    /// container's bounds; layout is unchanged, so they may still overflow it.
    pub fn set_clip(&self, clip: bool) {
        self.change(|state, id| {
            let element = &mut state.tree.get_mut(id).unwrap().context;
            if element.clips != clip {
                element.clips = clip;
                state.geometry_dirty = true;
                state.repaint = true;
            }
            Ok(())
        })
    }
}

/// Checks a handle method's argument: panics with [`UiError::InvalidValue`]
/// unless `valid`, like [`Node::change`] on other misuse.
pub fn require(valid: bool) {
    if !valid {
        fail(UiError::InvalidValue.into());
    }
}
