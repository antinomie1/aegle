use crate::{
    Result, Ui, UiError,
    state::{Content, State, focus_policy},
};
use aegle_access::accesskit::{
    Action, ActionData, ActionRequest, Affine, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate,
};
use aegle_controls::Input;
use aegle_core::Dirty;
use aegle_text::TextError;
use aegle_types::Point;

impl Ui {
    /// Whether logical semantics changed since the last exported update.
    pub fn access_dirty(&self) -> bool {
        let state = self.state.borrow();
        state.topology_dirty
            || state.geometry_dirty
            || state.order.iter().any(|&id| {
                state
                    .tree
                    .dirty(id)
                    .is_ok_and(|dirty| dirty.intersects(Dirty::SEMANTICS))
            })
    }
    /// Exports current semantics from the same controls used for painting and input.
    /// Refresh is performed first so geometry and text-run ranges agree.
    pub fn accessibility(&self, initial: bool, title: &str) -> Result<TreeUpdate> {
        let repaint = self.refresh()?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        // Semantic inspection must not consume a host's pending paint request.
        state.repaint |= repaint;
        state.prepare_accessibility()?;
        Ok(state.export_accessibility(initial, title, 1.0))
    }
    #[cfg(any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    pub(crate) fn publish_accessibility(
        &self,
        title: &str,
        scale: f64,
        publish: impl FnOnce(&mut dyn FnMut(bool) -> TreeUpdate),
    ) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.prepare_accessibility()?;
        publish(&mut |initial| state.export_accessibility(initial, title, scale));
        Ok(())
    }
    /// Applies supported assistive actions through shared focus/control/editing paths.
    /// Returns false for unsupported/stale actions or selection during composition.
    /// Scroll actions do not change focus or the editor's composition state.
    /// ScrollIntoView currently accepts no hint; ScrollToPoint is unsupported.
    pub fn access_action(&self, request: ActionRequest) -> Result<bool> {
        if request.target_tree != TreeId::ROOT {
            return Ok(false);
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.rebuild_order();
        let Some(target) = state
            .order
            .iter()
            .copied()
            .find(|&id| state.tree.get(id).unwrap().context.access_id == request.target_node)
        else {
            return Ok(false);
        };
        if !state.usable(target) {
            return Ok(false);
        }
        let repaint = state.refresh()?;
        state.repaint |= repaint;
        if let Some(handled) = state.access_scroll(target, request.action, request.data.as_ref())? {
            return Ok(handled);
        }
        if focus_policy(target, state.tree.get(target).unwrap())
            != aegle_core::FocusPolicy::Focusable
        {
            return Ok(false);
        }
        match (request.action, request.data) {
            (Action::Focus, _) => state.set_focus(Some(target))?,
            (Action::Click, _)
                if matches!(
                    state.tree.get(target).unwrap().context.content,
                    Content::Button(..) | Content::Toggle(_)
                ) =>
            {
                state.dispatch(target, Input::Activate)?
            }
            (Action::Increment | Action::Decrement, _)
                if matches!(
                    state.tree.get(target).unwrap().context.content,
                    Content::Slider(_)
                ) =>
            {
                state.dispatch(
                    target,
                    if request.action == Action::Increment {
                        Input::Increment
                    } else {
                        Input::Decrement
                    },
                )?;
            }
            (Action::SetValue, Some(ActionData::NumericValue(value)))
                if value.is_finite()
                    && matches!(
                        state.tree.get(target).unwrap().context.content,
                        Content::Slider(_)
                    ) =>
            {
                state.dispatch(target, Input::SetValue(value))?;
            }
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                let fonts = std::rc::Rc::clone(&state.fonts);
                if let Content::Field(field) =
                    &mut state.tree.get_mut(target).unwrap().context.content
                {
                    match fonts
                        .borrow_mut()
                        .edit(field.editor_mut())
                        .select_accessibility(&selection)
                    {
                        Ok(()) => {}
                        Err(TextError::InvalidRange | TextError::CompositionActive) => {
                            return Ok(false);
                        }
                        Err(error) => return Err(error.into()),
                    }
                    field.editor_mut().break_undo_group();
                    state.ime_dirty = true;
                    state.ime_reset = true;
                    state.input_method = false;
                } else {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

impl State {
    fn prepare_accessibility(&self) -> Result {
        // These are precisely the fallible text bridge boundaries. Font-backed
        // geometry and finite origins make the subsequent exporter infallible.
        let mut remaining_ids = self.tree.len() as u64;
        for &id in &self.order {
            let element = &self.tree.get(id).unwrap().context;
            if let Content::Field(field) = &element.content {
                if field.editor().diagnostics().unshaped_bytes != 0 {
                    return Err(TextError::MissingFont.into());
                }
                if !element.scroll.x.is_finite() || !element.scroll.y.is_finite() {
                    return Err(UiError::InvalidValue.into());
                }
                // A layout run cannot outnumber scalar bytes plus its empty run.
                remaining_ids = remaining_ids
                    .checked_add(field.editor().display_text().len() as u64 + 1)
                    .ok_or(UiError::IdentityExhausted)?;
            }
        }
        self.next_access_id
            .checked_add(remaining_ids)
            .ok_or(UiError::IdentityExhausted)?;
        Ok(())
    }
    fn export_accessibility(&mut self, initial: bool, title: &str, scale: f64) -> TreeUpdate {
        let root = self.tree.get(self.root).unwrap().context.access_id;
        let focus = self
            .focus
            .current(&self.tree)
            .map(|id| self.tree.get(id).unwrap().context.access_id)
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
        for index in 0..self.order.len() {
            let id = self.order[index];
            if !initial
                && id != self.root
                && !self.tree.dirty(id).unwrap().intersects(Dirty::SEMANTICS)
            {
                continue;
            }
            let enabled = self.usable(id);
            #[cfg(not(feature = "motion"))]
            let appearance = self.appearance(id);
            #[cfg(feature = "motion")]
            let appearance = self.presented_appearance(id);
            let color = appearance
                .expect("refresh validated the appearance")
                .foreground;
            let [red, green, blue, alpha] = color.to_rgba();
            let foreground = aegle_access::accesskit::Color {
                red,
                green,
                blue,
                alpha,
            };
            let node_data = self.tree.get(id).unwrap();
            let bounds = node_data.bounds();
            let parent_scroll = self
                .tree
                .parent(id)
                .unwrap()
                .and_then(|parent| self.tree.get(parent))
                .filter(|parent| matches!(parent.context.content, Content::Scroll(_)))
                .map_or(Point::default(), |parent| parent.context.scroll);
            let scroll_limit = matches!(node_data.context.content, Content::Scroll(_))
                .then(|| self.scroll_limit(id));
            let mut node = Node::new(Role::GenericContainer);
            node.set_foreground_color(foreground);
            node.set_bounds(Rect::new(
                0.0,
                0.0,
                bounds.size.width.into(),
                bounds.size.height.into(),
            ));
            let offset = node_data.context.offset;
            node.set_transform(Affine::translate((
                f64::from(bounds.origin.x + offset.x - parent_scroll.x),
                f64::from(bounds.origin.y + offset.y - parent_scroll.y),
            )));
            if enabled && self.has_scroll_ancestor(id) {
                node.add_action(Action::ScrollIntoView);
            }
            node.set_children(
                self.tree
                    .children(id)
                    .unwrap()
                    .map(|child| self.tree.get(child).unwrap().context.access_id)
                    .collect::<Vec<_>>(),
            );
            let element = &mut self.tree.get_mut(id).unwrap().context;
            let (padding, scroll) = (element.inset(&self.theme), element.scroll);
            if !element.effective_visible {
                node.set_hidden();
            }
            if !enabled {
                node.set_disabled();
            }
            if !element.label.is_empty() {
                node.set_label(element.label.as_str());
            }
            match &mut element.content {
                Content::Container => {
                    if id == self.root {
                        // Native adapters request physical coordinates through
                        // one root transform; descendant geometry stays logical.
                        node.set_transform(Affine::scale(scale));
                        node.set_role(Role::Window);
                        node.set_label(title);
                    }
                }
                Content::Scroll(_) => {
                    node.set_role(Role::ScrollView);
                    node.set_clips_children();
                    // Hidden layout is zeroed by Taffy while retained offsets are
                    // preserved. Publish their ranges again after visible layout.
                    if element.effective_visible {
                        let limit = scroll_limit.unwrap();
                        node.set_scroll_x(element.scroll.x.into());
                        node.set_scroll_x_min(0.0);
                        node.set_scroll_x_max(limit.x.into());
                        node.set_scroll_y(element.scroll.y.into());
                        node.set_scroll_y_min(0.0);
                        node.set_scroll_y_max(limit.y.into());
                        if enabled {
                            node.add_action(Action::SetScrollOffset);
                            if limit.x > 0.0 {
                                node.add_action(Action::ScrollLeft);
                                node.add_action(Action::ScrollRight);
                            }
                            if limit.y > 0.0 {
                                node.add_action(Action::ScrollUp);
                                node.add_action(Action::ScrollDown);
                            }
                        }
                    }
                }
                Content::Label(label) => {
                    node.set_role(Role::Label);
                    node.set_value(label.text());
                }
                Content::Button(_, label) => {
                    node.set_role(Role::Button);
                    if element.label.is_empty() {
                        node.set_label(label.text());
                    }
                    if enabled {
                        node.add_action(Action::Focus);
                        node.add_action(Action::Click);
                    }
                }
                Content::Toggle(toggle) => {
                    node.set_role(if toggle.switch {
                        Role::Switch
                    } else {
                        Role::CheckBox
                    });
                    node.set_toggled(if toggle.control.is_checked() {
                        aegle_access::accesskit::Toggled::True
                    } else {
                        aegle_access::accesskit::Toggled::False
                    });
                    if element.label.is_empty() {
                        node.set_label(toggle.text.text());
                    }
                    if enabled {
                        node.add_action(Action::Focus);
                        node.add_action(Action::Click);
                    }
                }
                Content::Slider(slider) => {
                    node.set_role(Role::Slider);
                    node.set_orientation(aegle_access::accesskit::Orientation::Horizontal);
                    numeric(&mut node, slider.range());
                    let range = slider.range();
                    node.set_numeric_value_step(if range.step() == 0.0 {
                        (range.max() - range.min()) / 100.0
                    } else {
                        range.step()
                    });
                    if enabled {
                        node.add_action(Action::Focus);
                        node.add_action(Action::SetValue);
                        node.add_action(Action::Increment);
                        node.add_action(Action::Decrement);
                    }
                }
                Content::Progress(range) => {
                    node.set_role(Role::ProgressIndicator);
                    numeric(&mut node, range);
                }
                Content::Image(_) => node.set_role(Role::Image),
                Content::Canvas(_) => node.set_role(Role::Canvas),
                Content::Field(field) => {
                    node.set_clips_children();
                    if enabled {
                        node.add_action(Action::Focus);
                    }
                    let start = update.nodes.len();
                    self.fonts
                        .borrow_mut()
                        .edit(field.editor_mut())
                        .accessibility(
                            &mut update,
                            &mut node,
                            || {
                                let id = NodeId(self.next_access_id);
                                self.next_access_id += 1;
                                id
                            },
                            Point::new(padding - scroll.x, padding - scroll.y),
                        )
                        .expect("prepared text and geometry satisfy the accessibility boundary");
                    // Painting overrides the retained shaping brush on palette
                    // changes; semantic text must report that same visible color.
                    for (_, run) in &mut update.nodes[start..] {
                        run.set_foreground_color(foreground);
                    }
                    if !enabled {
                        node.remove_action(Action::SetTextSelection);
                    }
                }
            }
            update.nodes.push((element.access_id, node));
            self.tree.clear_dirty(id, Dirty::SEMANTICS).unwrap();
        }
        update
    }
}

fn numeric(node: &mut Node, range: &aegle_controls::Range) {
    node.set_numeric_value(range.value());
    node.set_min_numeric_value(range.min());
    node.set_max_numeric_value(range.max());
}
