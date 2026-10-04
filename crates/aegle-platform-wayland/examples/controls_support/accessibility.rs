use super::{App, Content, INSET, Result, focus_policy};
use aegle_access::accesskit::{
    Action, ActionData, ActionRequest, Affine, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate,
};
use aegle_controls::Input;
use aegle_core::Dirty;
use aegle_text::TextError;
use aegle_types::Point;

impl App {
    fn access_id(&self, id: aegle_core::NodeId) -> NodeId {
        // This fixed example has six logical nodes. Text-run IDs start at 7;
        // a dynamic app must allocate semantic identities at node creation.
        if id == self.root {
            NodeId(1)
        } else {
            NodeId(2 + self.leaves.iter().position(|&leaf| leaf == id).unwrap() as u64)
        }
    }

    pub fn access_dirty(&self) -> bool {
        std::iter::once(self.root)
            .chain(self.leaves)
            .any(|id| self.tree.dirty(id).unwrap().intersects(Dirty::SEMANTICS))
    }

    /// Called after successful refresh, so layout, font availability and
    /// text origins are already validated by the rendering path.
    pub fn accessibility(&mut self, initial: bool) -> TreeUpdate {
        let root = self.access_id(self.root);
        let focus = self
            .focus
            .current(&self.tree)
            .map(|id| self.access_id(id))
            .unwrap_or(root);
        let mut update = TreeUpdate {
            nodes: Vec::new(),
            tree: None,
            tree_id: TreeId::ROOT,
            focus,
        };
        if initial {
            let mut tree = Tree::new(root);
            tree.toolkit_name = Some("Aegle".into());
            tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
            update.tree = Some(tree);
        }
        for id in std::iter::once(self.root).chain(self.leaves) {
            if !initial && !self.tree.dirty(id).unwrap().intersects(Dirty::SEMANTICS) {
                continue;
            }
            let semantic_id = self.access_id(id);
            let bounds = self.bounds(id);
            let mut node = Node::new(Role::GenericContainer);
            node.set_bounds(Rect::new(
                0.0,
                0.0,
                bounds.size.width.into(),
                bounds.size.height.into(),
            ));
            node.set_transform(Affine::translate((
                f64::from(bounds.origin.x),
                f64::from(bounds.origin.y),
            )));
            if id == self.root {
                node.set_role(Role::Window);
                node.set_label("Aegle retained controls");
                node.set_children(self.leaves.map(|id| self.access_id(id)));
            } else {
                match &mut self.tree.get_mut(id).unwrap().context.content {
                    Content::Label(label) => {
                        node.set_role(Role::Label);
                        node.set_value(label.text());
                    }
                    Content::Button(button, label) => {
                        node.set_role(Role::Button);
                        node.set_label(label.text());
                        if button.is_enabled() {
                            node.add_action(Action::Focus);
                            node.add_action(Action::Click);
                        } else {
                            node.set_disabled();
                        }
                    }
                    Content::Field(field) => {
                        node.set_label("Text editor");
                        node.set_clips_children();
                        if field.is_enabled() {
                            node.add_action(Action::Focus);
                        } else {
                            node.set_disabled();
                        }
                        self.fonts
                            .edit(field.editor_mut())
                            .accessibility(
                                &mut update,
                                &mut node,
                                || {
                                    let id = NodeId(self.next_access_id);
                                    self.next_access_id = self
                                        .next_access_id
                                        .checked_add(1)
                                        .expect("semantic identity exhausted");
                                    id
                                },
                                Point::new(INSET, INSET - self.scroll),
                            )
                            .expect("refresh established valid text and geometry");
                    }
                    Content::Container => unreachable!(),
                }
            }
            update.nodes.push((semantic_id, node));
            self.tree.clear_dirty(id, Dirty::SEMANTICS).unwrap();
        }
        update
    }

    pub fn access_action(&mut self, request: ActionRequest) -> Result<()> {
        if request.target_tree != TreeId::ROOT {
            return Ok(());
        }
        let Some(target) = [self.field, self.button]
            .into_iter()
            .find(|&id| self.access_id(id) == request.target_node)
        else {
            return Ok(());
        };
        if focus_policy(target, self.tree.get(target).unwrap())
            != aegle_core::FocusPolicy::Focusable
        {
            return Ok(());
        }
        match (request.action, request.data) {
            (Action::Focus, _) => {
                let change = self
                    .focus
                    .set(&self.tree, self.root, Some(target), focus_policy)?;
                self.change_focus(change)?;
            }
            (Action::Click, _) if target == self.button => {
                self.dispatch(target, Input::Activate)?
            }
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection)))
                if target == self.field =>
            {
                let Content::Field(field) = &mut self.tree.get_mut(target).unwrap().context.content
                else {
                    unreachable!()
                };
                let mut editor = self.fonts.edit(field.editor_mut());
                match editor.select_accessibility(&selection) {
                    Ok(()) => {
                        field.editor_mut().break_undo_group();
                        self.ime_sync = true;
                        self.ime_reset = true;
                        self.cause = aegle_platform_wayland::ImeCause::Other;
                    }
                    Err(TextError::InvalidRange | TextError::CompositionActive) => {
                        eprintln!(
                            "accessibility selection rejected: stale range or active IME composition"
                        );
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            _ => {}
        }
        Ok(())
    }
}
