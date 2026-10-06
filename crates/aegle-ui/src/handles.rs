use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use aegle_core::{Dirty, NodeId};
use aegle_layout::{
    Dimension, Edges, FlexDirection, LengthPercentage, LengthPercentageAuto, Style,
};
use aegle_types::{Rect, Size};

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
    fn layout(&self, local: u8, change: impl FnOnce(&mut Style)) -> Result {
        self.change(|state, id| {
            let mut style = state.tree.get(id).unwrap().style().clone();
            change(&mut style);
            state.tree.get_mut(id).unwrap().context.local_layout |= local;
            aegle_layout::set_style(&mut state.tree, id, style)?;
            Ok(())
        })
    }
    /// Sets explicit logical dimensions; `None` restores automatic sizing.
    pub fn set_size(&self, width: Option<f32>, height: Option<f32>) -> Result {
        for value in [width, height].into_iter().flatten() {
            valid(value)?;
        }
        self.layout(1, |s| {
            s.size = aegle_layout::Size {
                width: width.map_or(Dimension::auto(), Dimension::length),
                height: height.map_or(Dimension::auto(), Dimension::length),
            }
        })
    }
    /// Sets the logical width, preserving height and its theme default.
    /// `None` restores automatic width.
    pub fn set_width(&self, width: Option<f32>) -> Result {
        if let Some(width) = width {
            valid(width)?;
        }
        self.layout(0, |s| {
            s.size.width = width.map_or(Dimension::auto(), Dimension::length)
        })
    }
    /// Sets the logical height; `None` selects automatic rather than themed height.
    pub fn set_height(&self, height: Option<f32>) -> Result {
        if let Some(height) = height {
            valid(height)?;
        }
        self.layout(1, |s| {
            s.size.height = height.map_or(Dimension::auto(), Dimension::length)
        })
    }
    /// Sets nonnegative minimum logical dimensions.
    pub fn set_min_size(&self, size: Size) -> Result {
        valid(size.width)?;
        valid(size.height)?;
        self.layout(8, |s| {
            s.min_size = aegle_layout::Size {
                width: LengthPercentageAuto::length(size.width),
                height: LengthPercentageAuto::length(size.height),
            }
        })
    }
    /// Sets minimum logical width without changing the minimum height.
    pub fn set_min_width(&self, width: f32) -> Result {
        valid(width)?;
        self.layout(0, |s| {
            s.min_size.width = LengthPercentageAuto::length(width)
        })
    }
    /// Sets minimum logical height, overriding the corresponding theme default.
    pub fn set_min_height(&self, height: f32) -> Result {
        valid(height)?;
        self.layout(8, |s| {
            s.min_size.height = LengthPercentageAuto::length(height)
        })
    }
    /// Sets a finite nonnegative flex grow factor; zero keeps intrinsic sizing.
    pub fn set_grow(&self, grow: f32) -> Result {
        valid(grow)?;
        self.layout(0, |s| s.flex_grow = grow)
    }
    /// Sets uniform nonnegative content padding.
    pub fn set_padding(&self, padding: f32) -> Result {
        valid(padding)?;
        self.change(|state, id| {
            let node = state.tree.get_mut(id).unwrap();
            if matches!(
                node.context.control.kind(),
                aegle_theme::ControlKind::Container | aegle_theme::ControlKind::ScrollView
            ) {
                node.context.local_layout |= 2;
                let mut style = node.style().clone();
                let p = LengthPercentage::length(padding);
                style.padding = Edges {
                    left: p,
                    right: p,
                    top: p,
                    bottom: p,
                };
                aegle_layout::set_style(&mut state.tree, id, style)?;
            } else {
                node.context.padding = Some(padding);
                state.tree.mark_dirty(id, Dirty::ALL)?;
            }
            Ok(())
        })
    }
    /// Sets horizontal and vertical spacing between children.
    pub fn set_gap(&self, gap: f32) -> Result {
        valid(gap)?;
        self.layout(4, |s| {
            s.gap = aegle_layout::Size {
                width: LengthPercentage::length(gap),
                height: LengthPercentage::length(gap),
            }
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
}

/// Rejects nonfinite or negative lengths.
pub fn valid(value: f32) -> Result {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(UiError::InvalidValue.into())
    }
}
