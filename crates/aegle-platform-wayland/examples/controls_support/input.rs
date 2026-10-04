use super::{App, Content, INSET, Result, focus_policy};
use aegle_controls::{
    Action, Capture, Input, Key, KeyInput, Modifiers, Outcome, PointerId, PointerInput, PointerKind,
};
use aegle_core::{Dirty, EventControl, EventPhase, FocusChange, FocusDirection, NodeId};
use aegle_platform_wayland::{ImeCause, ImeUpdate, KeyEvent, Keysym};
use aegle_text::{ImeEdit, Selection};
use aegle_types::Point;

impl App {
    pub fn window_focus(&mut self, focused: bool) -> Result<()> {
        if focused && self.focus.current(&self.tree).is_some() {
            return Ok(());
        }
        self.cancel_pointer()?;
        let change = if focused {
            self.focus.advance(
                &self.tree,
                self.root,
                FocusDirection::Forward,
                true,
                focus_policy,
            )?
        } else {
            self.focus.set(&self.tree, self.root, None, focus_policy)?
        };
        self.change_focus(change)
    }

    pub fn key(
        &mut self,
        key: KeyEvent,
        modifiers: aegle_platform_wayland::Modifiers,
        pressed: bool,
        repeat: bool,
    ) -> Result<()> {
        self.modifiers = modifiers.into_controls();
        let key_id = match key.keysym {
            Keysym::Return | Keysym::KP_Enter => Key::Enter,
            Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
            Keysym::Escape => Key::Escape,
            Keysym::BackSpace => Key::Backspace,
            Keysym::Delete => Key::Delete,
            Keysym::Left => Key::Left,
            Keysym::Right => Key::Right,
            Keysym::Up => Key::Up,
            Keysym::Down => Key::Down,
            Keysym::Home => Key::Home,
            Keysym::End => Key::End,
            value => value
                .key_char()
                .map(Key::Character)
                .unwrap_or(Key::Unidentified),
        };
        let target = self.focus.current(&self.tree).unwrap_or(self.field);
        self.dispatch(
            target,
            Input::Key(KeyInput {
                key: key_id,
                text: key.utf8.as_deref().unwrap_or(""),
                modifiers: self.modifiers,
                pressed,
                repeat,
            }),
        )
    }

    pub fn pointer(&mut self, id: PointerId, kind: PointerKind, position: Point) -> Result<()> {
        let hit = [self.field, self.button]
            .into_iter()
            .find(|&node| self.bounds(node).contains(position));
        let target = self
            .capture
            .filter(|(pointer, _)| *pointer == id)
            .map(|(_, node)| node)
            .or(hit);
        if self.hover != hit {
            if let Some(old) = self.hover.take() {
                self.dispatch(
                    old,
                    self.pointer_input(old, id, PointerKind::Leave, position),
                )?;
            }
            self.hover = hit;
        }
        if let Some(target) = target {
            self.dispatch(target, self.pointer_input(target, id, kind, position))?;
        }
        Ok(())
    }

    pub fn pointer_leave(&mut self) -> Result<()> {
        // A surface leave or capability removal ends this demo's gesture.
        self.cancel_pointer()
    }

    pub fn scroll(&mut self, position: Point, amount: f32) -> Result<()> {
        if self.bounds(self.field).contains(position) {
            self.scroll = (self.scroll + amount).max(0.0);
            self.tree
                .mark_dirty(self.field, Dirty::PAINT | Dirty::SEMANTICS)?;
            self.ime_sync = true;
        }
        Ok(())
    }

    pub fn ime(&mut self, update: ImeUpdate) -> Result<()> {
        if let Some(target) = self.focus.current(&self.tree) {
            self.dispatch(
                target,
                Input::Ime(ImeEdit {
                    delete_before: update.delete_before as usize,
                    delete_after: update.delete_after as usize,
                    commit: update.commit.as_deref(),
                    preedit: &update.preedit.text,
                    cursor: update
                        .preedit
                        .cursor
                        .map(|(anchor, focus)| Selection { anchor, focus }),
                }),
            )?;
            self.ime_sync = true;
            self.cause = ImeCause::InputMethod;
        }
        Ok(())
    }

    pub fn ime_left(&mut self) -> Result<()> {
        if let Some(target) = self.focus.current(&self.tree) {
            self.dispatch(target, Input::Cancel)?;
        }
        self.ime_sync = true;
        Ok(())
    }

    /// The route snapshots topology, so listeners and actions hold no tree borrow.
    pub(super) fn dispatch(&mut self, target: NodeId, input: Input<'_>) -> Result<()> {
        let mut route = std::mem::take(&mut self.route);
        route.rebuild(&self.tree, self.root, target)?;
        let mut event = EventControl::default();
        let mut action = None;
        for step in route.iter() {
            if self.tree.get(step.node).is_none() {
                continue;
            }
            match step.phase {
                EventPhase::Capture if step.node == self.root => {
                    if let Input::Key(key) = input {
                        if key.key == Key::Tab && key.pressed {
                            let direction = if key.modifiers.shift {
                                FocusDirection::Backward
                            } else {
                                FocusDirection::Forward
                            };
                            let change = self.focus.advance(
                                &self.tree,
                                self.root,
                                direction,
                                true,
                                focus_policy,
                            )?;
                            self.change_focus(change)?;
                            event.prevent_default();
                        }
                    }
                }
                EventPhase::Target if !event.is_default_prevented() => {
                    let outcome = self.control(target, input)?;
                    action = outcome.action;
                    self.effects(target, outcome)?;
                }
                EventPhase::Bubble if step.node == self.root => {
                    if let Some(action) = action {
                        self.application_action(target, action)?;
                        event.stop_propagation();
                    }
                }
                _ => {}
            }
            if event.is_propagation_stopped() {
                break;
            }
        }
        self.route = route;
        if !matches!(input, Input::Ime(_)) {
            self.cause = ImeCause::Other;
        }
        Ok(())
    }

    fn control(&mut self, target: NodeId, input: Input<'_>) -> Result<Outcome> {
        Ok(
            match &mut self.tree.get_mut(target).unwrap().context.content {
                Content::Field(field) => field.handle(&mut self.fonts, input)?,
                Content::Button(button, _) => button.handle(input),
                _ => Outcome::default(),
            },
        )
    }

    fn effects(&mut self, target: NodeId, outcome: Outcome) -> Result<()> {
        if outcome.repaint {
            self.tree.mark_dirty(target, Dirty::PAINT)?;
        }
        if outcome.semantics {
            self.tree.mark_dirty(target, Dirty::SEMANTICS)?;
        }
        self.ime_reset |= outcome.reset_ime;
        self.ime_sync |= outcome.reset_ime;
        if outcome.focus {
            let change = self
                .focus
                .set(&self.tree, self.root, Some(target), focus_policy)?;
            self.change_focus(change)?;
        }
        match outcome.capture {
            Some(Capture::Acquire(id)) => self.capture = Some((id, target)),
            Some(Capture::Release(id)) if self.capture == Some((id, target)) => self.capture = None,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn change_focus(&mut self, change: FocusChange) -> Result<()> {
        if !change.changed() {
            return Ok(());
        }
        if let Some(previous) = change.previous {
            let outcome = self.control(previous, Input::Focus(false))?;
            self.effects(previous, outcome)?;
        }
        // Even a focus loss without a visible preedit invalidates the old
        // protocol session before publishing the newly focused control state.
        self.ime_reset = true;
        if let Some(current) = change.current {
            let outcome = self.control(current, Input::Focus(true))?;
            self.effects(current, outcome)?;
        }
        self.ime_sync = true;
        Ok(())
    }

    fn application_action(&mut self, target: NodeId, action: Action) -> Result<()> {
        if target == self.button && action == Action::Activate {
            // Ordinary application callback: mutates another retained control.
            let Content::Field(field) = &mut self.tree.get_mut(self.field).unwrap().context.content
            else {
                unreachable!()
            };
            let mut edit = self.fonts.edit(field.editor_mut());
            edit.cancel_preedit();
            edit.set_text("")?;
            self.ime_reset = true;
            self.ime_sync = true;
            println!("routed button action: text cleared");
        }
        Ok(())
    }

    fn pointer_input(
        &self,
        target: NodeId,
        id: PointerId,
        kind: PointerKind,
        position: Point,
    ) -> Input<'static> {
        let bounds = self.bounds(target);
        let mut local = Point::new(position.x - bounds.origin.x, position.y - bounds.origin.y);
        if target == self.field {
            local.x -= INSET;
            local.y += self.scroll - INSET;
        }
        Input::Pointer(PointerInput {
            id,
            kind,
            position: local,
            inside: bounds.contains(position),
            modifiers: self.modifiers,
        })
    }

    fn cancel_pointer(&mut self) -> Result<()> {
        if let Some((_, target)) = self.capture.take() {
            self.dispatch(target, Input::Cancel)?;
        }
        if let Some(target) = self.hover.take() {
            self.dispatch(
                target,
                Input::Pointer(PointerInput {
                    id: PointerId(0),
                    kind: PointerKind::Leave,
                    position: Point::default(),
                    inside: false,
                    modifiers: self.modifiers,
                }),
            )?;
        }
        Ok(())
    }
}

pub(super) trait NormalizeModifiers {
    fn into_controls(self) -> Modifiers;
}
impl NormalizeModifiers for aegle_platform_wayland::Modifiers {
    fn into_controls(self) -> Modifiers {
        Modifiers {
            shift: self.shift,
            control: self.ctrl,
            alt: self.alt,
            meta: self.logo,
        }
    }
}
