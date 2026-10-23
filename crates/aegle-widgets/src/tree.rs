//! A hierarchical tree view with expandable and collapsible nodes.

use std::cell::RefCell;

use aegle_controls::{Action, Button, Input, Key, Outcome, PointerKind};
use aegle_core::NodeId;
use aegle_layout::{Dimension, Style};
use aegle_scene::Affine;
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::{Color, Size};
use aegle_ui::{
    Container, Control, HandlerResult, Node, OrFail, Result,
    control::{ControlVisual, Frame, InputCx, MeasureCx, PaintCx},
    handle, text_style,
};

use crate::paint::{CHEVRON, ChevronDirection, chevron_dir, chosen_bar};

/// The control inside a [`TreeNode`] header row.
pub struct TreeNodeControl {
    button: Button,
    text: Paragraph,
    pub(crate) expanded: bool,
    pub(crate) selected: bool,
    pub(crate) has_children: bool,
    pub(crate) depth: usize,
    pub(crate) children: Option<NodeId>,
}

impl TreeNodeControl {
    /// Creates a new tree node header control with given label and nesting depth.
    pub fn new(
        fonts: &RefCell<TextSystem>,
        text: &str,
        depth: usize,
        theme: &Theme,
    ) -> Result<Self> {
        Ok(Self {
            button: Button::new(),
            text: fonts.borrow_mut().paragraph(text, &text_style(theme))?,
            expanded: false,
            selected: false,
            has_children: false,
            depth,
            children: None,
        })
    }

    /// Indentation from the left edge in logical pixels.
    fn indent(&self) -> f32 {
        self.depth as f32 * 16.0
    }
}

impl Control for TreeNodeControl {
    fn kind(&self) -> &'static ControlKind {
        &crate::kinds::TREE_NODE
    }
    fn interactive(&self) -> bool {
        true
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn visual(&self) -> ControlVisual {
        ControlVisual {
            pressed: self.button.is_pressed(),
            hovered: Some(self.button.is_hovered()),
            checked: self.selected,
            ..Default::default()
        }
    }
    fn paragraph(&self) -> Option<&Paragraph> {
        Some(&self.text)
    }
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        Some(&mut self.text)
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Outcome {
        let mut outcome = self.button.handle(input);
        match input {
            Input::Pointer(pointer) => {
                if let PointerKind::Down { .. } = pointer.kind {
                    let chevron_area = self.indent() + 20.0;
                    if pointer.position.x <= chevron_area && self.has_children {
                        self.expanded = !self.expanded;
                        outcome.action = Some(Action::Change);
                        if let Some(children_id) = self.children {
                            let expanded = self.expanded;
                            cx.deferred.push(Box::new(move |state, _| {
                                state.set_visible(children_id, expanded)
                            }));
                        }
                    } else {
                        self.selected = true;
                        outcome.action = Some(Action::Activate);
                    }
                }
            }
            Input::Key(key) if key.pressed => match key.key {
                Key::Right if self.has_children && !self.expanded => {
                    self.expanded = true;
                    outcome.action = Some(Action::Change);
                    if let Some(children_id) = self.children {
                        let expanded = self.expanded;
                        cx.deferred.push(Box::new(move |state, _| {
                            state.set_visible(children_id, expanded)
                        }));
                    }
                }
                Key::Left if self.has_children && self.expanded => {
                    self.expanded = false;
                    outcome.action = Some(Action::Change);
                    if let Some(children_id) = self.children {
                        let expanded = self.expanded;
                        cx.deferred.push(Box::new(move |state, _| {
                            state.set_visible(children_id, expanded)
                        }));
                    }
                }
                Key::Enter | Key::Character(' ') if outcome.action == Some(Action::Activate) => {
                    self.selected = true;
                }
                _ => {}
            },
            _ => {}
        }
        outcome
    }
    fn hover(&mut self, cx: &mut InputCx<'_>, _: aegle_ui::PointerId, input: Input<'_>) -> Outcome {
        self.handle(cx, input)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Size {
        let indent = self.indent();
        let chevron_width = if self.has_children {
            CHEVRON + cx.gap
        } else {
            CHEVRON
        };
        let text_size = self.text.reflow(None, cx.alignment()).or_fail();
        Size::new(
            indent + chevron_width + text_size.width + 2.0 * cx.padding,
            text_size.height.max(24.0) + 2.0 * cx.padding,
        )
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let size = cx.size;
        cx.builder.push_clip(cx.shape);
        // The selected row is tinted and marked by a leading accent bar, not
        // only by color; hover and press layer over either.
        if self.selected {
            cx.builder.fill(cx.shape, cx.appearance.selection);
            chosen_bar(cx.builder, size, cx.rtl, cx.appearance.indicator);
        }
        if cx.appearance.background != Color::TRANSPARENT {
            cx.builder.fill(cx.shape, cx.appearance.background);
        }

        let indent = self.indent();
        let center_y = size.height * 0.5;

        // Paint chevron if this node has child nodes.
        if self.has_children {
            let chevron_x = indent + CHEVRON * 0.5 + cx.padding;
            let dir = if self.expanded {
                ChevronDirection::Down
            } else {
                ChevronDirection::Right
            };
            chevron_dir(
                cx.builder,
                chevron_x,
                center_y,
                dir,
                cx.appearance.foreground,
            );
        }

        // Paint text label.
        let text_x = indent + CHEVRON + cx.theme.gap + cx.padding;
        let text_y = (size.height - self.text.size().height) * 0.5;
        cx.builder
            .push_transform(Affine::translation(text_x, text_y));
        self.text
            .paint_with_color(cx.builder, cx.appearance.foreground);
        cx.builder.pop();

        cx.builder.pop();
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::Role;
        cx.node.set_role(Role::TreeItem);
        cx.node.set_selected(self.selected);
        if self.has_children {
            cx.node.set_expanded(self.expanded);
        }
        cx.node.set_value(self.text.text());
    }
}

handle! {
    /// A single expandable node within a [`TreeView`].
    pub TreeNode(TreeNodeControl)
}

impl TreeNode {
    /// Appends a nested child node under this tree node.
    pub fn node(&self, text: &str) -> TreeNode {
        let depth = self.read(|control| control.depth) + 1;
        self.update(|control| control.has_children = true);

        let children = self.children_container();
        create_tree_node(&children, text, depth)
    }

    /// Replaces the node label text.
    pub fn set_text(&self, text: &str) {
        self.change(|state, id| state.set_text(id, text))
    }

    /// Returns the current node label text.
    pub fn text(&self) -> String {
        self.change(|state, id| state.text(id))
    }

    /// Sets whether the node is expanded to reveal its children.
    pub fn set_expanded(&self, expanded: bool) {
        let changed = self.read(|control| control.expanded != expanded);
        if changed {
            let children_id = self.read(|control| control.children);
            self.update(|control| control.expanded = expanded);
            if let Some(children_id) = children_id {
                self.0
                    .change(|state, _| state.set_visible(children_id, expanded));
            }
        }
    }

    /// Returns whether this tree node is currently expanded.
    pub fn is_expanded(&self) -> bool {
        self.read(|control| control.expanded)
    }

    /// Toggles the node's expanded state.
    pub fn toggle(&self) {
        self.set_expanded(!self.is_expanded());
    }

    /// Sets whether this node is marked as selected.
    pub fn set_selected(&self, selected: bool) {
        self.update(|control| control.selected = selected);
    }

    /// Returns whether this node is currently selected.
    pub fn is_selected(&self) -> bool {
        self.read(|control| control.selected)
    }

    /// Returns the children container where nested tree nodes are stored.
    pub fn children_container(&self) -> Container {
        let children_id = self.read(|control| control.children).unwrap_or(self.0.id);
        Container(Node {
            state: self.0.state.clone(),
            id: children_id,
        })
    }

    /// Appends a node to `parent`: a [`TreeView`], or the
    /// [`children_container`](Self::children_container) of the node it then
    /// nests under, directly or through transparent contents groups such as
    /// markup `if` and `for` blocks. Markup elements receive only that container.
    pub fn append_to(parent: &Container, text: &str) -> TreeNode {
        let owner = parent.change(|state, mut id| {
            while state.tree.get(id).unwrap().is_contents() {
                id = state.tree.parent(id)?.unwrap();
            }
            let Some(item) = state.tree.parent(id)? else {
                return Ok(None);
            };
            let Some(header) = state.tree.child(item, 0)? else {
                return Ok(None);
            };
            let owns = state
                .control_as::<TreeNodeControl>(header)
                .is_some_and(|control| control.children == Some(id));
            Ok(owns.then_some(header))
        });
        let depth = owner.map_or(0, |id| {
            let owner = TreeNode(Node {
                state: parent.state.clone(),
                id,
            });
            owner.update(|control| control.has_children = true);
            owner.read(|control| control.depth) + 1
        });
        create_tree_node(parent, text, depth)
    }

    /// Adds a selection handler invoked when the node is clicked or chosen.
    pub fn on_select<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(TreeNode) -> R + 'static,
    ) -> aegle_ui::Subscription {
        let node = self.clone();
        self.0
            .on_action(move |_| callback(node.clone()).into_result())
    }
}

handle! {
    /// A tree view container that displays hierarchical data nodes.
    pub TreeView
}

impl TreeView {
    /// Appends a root-level tree node.
    pub fn node(&self, text: &str) -> TreeNode {
        create_tree_node(&Container(self.0.clone()), text, 0)
    }
}

/// Helper function to construct a [`TreeNode`] with header row and children column.
fn create_tree_node(parent: &Container, text: &str, depth: usize) -> TreeNode {
    // The node item is a column containing the header row and the children column.
    let item_col = parent.column();

    let header_node = crate::add(&item_col, |state, theme| {
        let control = TreeNodeControl::new(&state.fonts, text, depth, theme)?;
        let style = Style {
            size: aegle_layout::Size {
                width: Dimension::percent(1.0),
                height: Dimension::length(theme.control_height),
            },
            ..Default::default()
        };
        Ok((Box::new(control) as Box<dyn Control>, style))
    });

    let children_col = item_col.column();
    // Child container is initially hidden until expanded.
    children_col.change(|state, id| state.set_visible(id, false));

    let tree_node = TreeNode(header_node);
    tree_node.update(|control| {
        control.children = Some(children_col.0.id);
    });

    tree_node
}

/// Creates a new [`TreeView`] inside `container`.
pub(crate) fn tree_view(container: &Container) -> TreeView {
    let col = container.column();
    col.change(|state, _| {
        state.install(&crate::HOOKS);
        Ok(())
    });
    TreeView(col.0)
}
