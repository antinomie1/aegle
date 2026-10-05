use std::{
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};

use aegle_core::{Dirty, NodeId};
use aegle_layout::{
    Dimension, Edges, FlexDirection, LengthPercentage, LengthPercentageAuto, Style,
};
use aegle_text::EditorOptions;
use aegle_types::{Rect, Size};

use crate::{
    Result, UiError,
    state::{Content, State},
    ui::container_style,
};

/// Weak generational identity. Cloning a handle neither copies nor owns its control.
#[derive(Clone)]
pub struct Node {
    pub(crate) state: Weak<RefCell<State>>,
    pub(crate) id: NodeId,
}

impl Node {
    pub(crate) fn change<T>(
        &self,
        change: impl FnOnce(&mut State, NodeId) -> Result<T>,
    ) -> Result<T> {
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
            state.cancel_subtree(id)?;
            state.tree.remove_with(id, |node, _| {
                state.callbacks.remove(&node);
                state.decorations.remove(&node);
            })?;
            state
                .pending
                .retain(|(id, _)| state.tree.get(*id).is_some());
            state.invalidate_structure();
            Ok(())
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
            Ok(())
        })
    }
    /// Shows or hides the entire subtree. Hidden controls take no layout space.
    pub fn set_visible(&self, visible: bool) -> Result {
        self.change(|state, id| {
            if state.tree.get(id).unwrap().context.visible == visible {
                return Ok(());
            }
            if !visible {
                state.cancel_subtree(id)?;
            }
            state.tree.get_mut(id).unwrap().context.visible = visible;
            let mut style = state.tree.get(id).unwrap().style().clone();
            style.display = if visible {
                aegle_layout::Display::Flex
            } else {
                aegle_layout::Display::None
            };
            aegle_layout::set_style(&mut state.tree, id, style)?;
            state.repaint = true;
            Ok(())
        })
    }
    /// Disables interaction throughout this subtree, preserving its displayed values.
    pub fn set_enabled(&self, enabled: bool) -> Result {
        self.change(|state, id| {
            if !enabled {
                state.cancel_subtree(id)?;
            }
            state.tree.get_mut(id).unwrap().context.enabled = enabled;
            let fonts = Rc::clone(&state.fonts);
            let outcome = match &mut state.tree.get_mut(id).unwrap().context.content {
                Content::Button(button, _) => button.set_enabled(enabled),
                Content::Field(field) => field.set_enabled(&mut fonts.borrow_mut(), enabled),
                _ => Default::default(),
            };
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
            if matches!(node.context.content, Content::Container) {
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

macro_rules! handle {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone)]
        pub struct $name(pub(crate) Node);
        impl Deref for $name {
            type Target = Node;
            fn deref(&self) -> &Node {
                &self.0
            }
        }
    };
}
handle!(
    Container,
    "A retained row or column. Creation methods append children once."
);
handle!(Label, "A retained display paragraph.");
handle!(
    Button,
    "A retained button with shared pointer, keyboard and semantic activation."
);
handle!(
    TextField,
    "A retained plain text editor, including native IME composition state."
);

impl Container {
    fn add(&self, create: impl FnOnce(&mut State) -> Result<(Content, Style)>) -> Result<Node> {
        self.change(|state, parent| {
            let (content, style) = create(state)?;
            let id = state.insert(parent, content, style)?;
            Ok(Node {
                state: self.state.clone(),
                id,
            })
        })
    }
    /// Appends a vertical container.
    pub fn column(&self) -> Result<Container> {
        self.add(|state| Ok((Content::Container, container_style(&state.theme, false))))
            .map(Container)
    }
    /// Appends a horizontal container.
    pub fn row(&self) -> Result<Container> {
        self.add(|state| {
            let mut style = container_style(&state.theme, false);
            style.flex_direction = FlexDirection::Row;
            Ok((Content::Container, style))
        })
        .map(Container)
    }
    /// Appends a paragraph. Text wraps to available layout width.
    pub fn text(&self, text: &str) -> Result<Label> {
        self.add(|state| {
            Ok((
                Content::Label(Box::new(
                    state.fonts.borrow_mut().paragraph(text, &state.style())?,
                )),
                Style {
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            ))
        })
        .map(Label)
    }
    /// Appends a neutral button with its visible text as the default accessible name.
    pub fn button(&self, text: &str) -> Result<Button> {
        self.add(|state| {
            Ok((
                Content::Button(
                    aegle_controls::Button::new(),
                    Box::new(state.fonts.borrow_mut().paragraph(text, &state.style())?),
                ),
                Style {
                    size: aegle_layout::Size {
                        width: Dimension::auto(),
                        height: Dimension::length(state.theme.control_height),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            ))
        })
        .map(Button)
    }
    /// Appends a single-line editor. Enter produces a submit action.
    pub fn text_field(&self, text: &str) -> Result<TextField> {
        self.editor(text, false)
    }
    /// Appends a wrapping multiline editor with a default four-line viewport.
    pub fn text_area(&self, text: &str) -> Result<TextField> {
        self.editor(text, true)
    }
    fn editor(&self, text: &str, multiline: bool) -> Result<TextField> {
        self.add(|state| {
            let editor = state.fonts.borrow_mut().editor(
                text,
                &state.style(),
                EditorOptions {
                    multiline,
                    ..Default::default()
                },
            )?;
            Ok((
                Content::Field(Box::new(aegle_controls::TextField::new(editor))),
                Style {
                    size: aegle_layout::Size {
                        width: Dimension::auto(),
                        height: Dimension::length(
                            state.theme.control_height * if multiline { 4.0 } else { 1.0 },
                        ),
                    },
                    min_size: aegle_layout::Size {
                        width: LengthPercentageAuto::length(0.0),
                        height: LengthPercentageAuto::length(state.theme.control_height),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            ))
        })
        .map(TextField)
    }
}

fn valid(value: f32) -> Result {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(UiError::InvalidValue.into())
    }
}
