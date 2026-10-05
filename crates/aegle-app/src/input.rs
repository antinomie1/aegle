use crate::{
    ClipboardRequest, Result, Ui, UiError,
    state::{Content, Mark, State, focus_policy},
};
use aegle_controls::{
    Action, Capture, Clipboard, Input, Key, KeyInput, Modifiers, Outcome, PointerId, PointerInput,
    PointerKind,
};
use aegle_core::{Dirty, EventPhase, FocusChange, FocusDirection, NodeId};
use aegle_text::ImeEdit;
use aegle_types::Point;

impl Ui {
    /// Updates native window activation. Losing activation cancels capture and IME.
    pub fn window_focus(&self, focused: bool) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if focused && state.focus.current(&state.tree).is_some() {
            return Ok(());
        }
        state.cancel_pointer()?;
        if focused {
            if let Some(id) = state.last_focus.filter(|&id| state.usable(id)) {
                state.set_focus(Some(id))
            } else {
                state.advance_focus(FocusDirection::Forward)
            }
        } else {
            if let Some(id) = state.focus.current(&state.tree) {
                state.last_focus = Some(id);
            }
            state.set_focus(None)
        }
    }
    /// Delivers normalized keyboard input; Tab/Shift+Tab traverse logical controls.
    pub fn key(&self, key: KeyInput<'_>) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if key.pressed && (state.popup_key(&key)? || state.radio_key(&key)?) {
            return Ok(());
        }
        if key.key == Key::Tab
            && key.pressed
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.meta
        {
            return state.advance_focus(if key.modifiers.shift {
                FocusDirection::Backward
            } else {
                FocusDirection::Forward
            });
        }
        if let Some(id) = state.focus.current(&state.tree) {
            state.dispatch(id, Input::Key(key))?;
        }
        Ok(())
    }
    /// Delivers a primary pointer transition in logical window coordinates.
    pub fn pointer(
        &self,
        id: PointerId,
        kind: PointerKind,
        position: Point,
        modifiers: Modifiers,
    ) -> Result {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(UiError::InvalidValue.into());
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.rebuild_order();
        if matches!(kind, PointerKind::Down { .. }) {
            state.dismiss_popups(position)?;
        }
        state.pointer =
            (!matches!(kind, PointerKind::Leave | PointerKind::Cancel)).then_some((id, position));
        let hit = state.pointer.and_then(|_| state.hit(position));
        if state.scrollbar_pointer(id, kind, position, hit)? {
            return Ok(());
        }
        let target = state
            .capture
            .filter(|(pointer, _)| *pointer == id)
            .map(|(_, id)| id)
            .or(hit);
        state.update_hover(hit, id, position)?;
        if let Some(target) = target {
            let input = state.pointer_input(target, id, kind, position, modifiers);
            state.dispatch(target, input)?;
        }
        if target != hit {
            state.sync_hover(id, position)?;
        }
        Ok(())
    }
    /// Ends this surface's pointer gesture on leave or device removal.
    pub fn pointer_leave(&self) -> Result {
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .cancel_pointer()
    }
    /// Scrolls at a window point by a finite logical vertical displacement.
    /// Uses the same nested viewport/editor routing as [`Self::scroll_by`].
    pub fn scroll(&self, position: Point, amount: f32) -> Result {
        self.scroll_by(position, Point::new(0.0, amount))
    }
    /// Scrolls the nearest available viewport or editor under a window point.
    /// Coordinates and both displacement axes must be finite logical pixels.
    pub fn scroll_by(&self, position: Point, delta: Point) -> Result {
        if ![position.x, position.y, delta.x, delta.y]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(UiError::InvalidValue.into());
        }
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .scroll_by_at(position, delta)?;
        Ok(())
    }
    /// Applies one validated native IME transaction to the focused editable field.
    pub fn ime(&self, edit: ImeEdit<'_>) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if let Some(id) = state.focus.current(&state.tree) {
            state.dispatch(id, Input::Ime(edit))?;
            state.ime_dirty = true;
            state.input_method = true;
        }
        Ok(())
    }
    /// Delivers native clipboard text to the focused editor, replacing its
    /// selection. Single-line editors drop line breaks.
    pub fn paste(&self, text: &str) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if let Some(id) = state.focus.current(&state.tree) {
            state.dispatch(id, Input::Paste(text))?;
        }
        Ok(())
    }
    /// Cancels local composition when the native text-input focus leaves the window.
    pub fn ime_left(&self) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if let Some(id) = state.focus.current(&state.tree) {
            state.dispatch(id, Input::Cancel)?;
        }
        state.ime_dirty = true;
        Ok(())
    }
    /// Requests resending current editor state when a native input method enters.
    pub fn request_ime_sync(&self) -> Result {
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .ime_dirty = true;
        Ok(())
    }
}

impl State {
    pub fn dispatch(&mut self, target: NodeId, input: Input<'_>) -> Result {
        if !self.usable(target) {
            return Ok(());
        }
        let mut route = std::mem::take(&mut self.route);
        route.rebuild(&self.tree, self.root, target)?;
        let mut result = Ok(());
        for step in route.iter() {
            if step.phase == EventPhase::Target {
                result = self
                    .control(target, input)
                    .and_then(|outcome| self.effects(target, outcome));
            }
        }
        self.route = route;
        if !matches!(input, Input::Ime(_)) {
            self.input_method = false;
        }
        result
    }
    pub fn control(&mut self, target: NodeId, input: Input<'_>) -> Result<Outcome> {
        let element = &mut self.tree.get_mut(target).unwrap().context;
        let (_, extent) =
            aegle_widgets::slider_track(element.bounds.size, element.inset(&self.theme));
        let mut radio = false;
        let outcome = match &mut element.content {
            Content::Button(button, _) => button.handle(input),
            Content::Toggle(toggle) => {
                let was = toggle.control.is_checked();
                let mut outcome = toggle.control.handle(input);
                if outcome.action == Some(Action::Change) {
                    if toggle.mark == Mark::Radio {
                        // Choosing the chosen radio button changes nothing.
                        toggle.control.set_checked(true);
                        radio = !was;
                        outcome.action = radio.then_some(Action::Change);
                    } else if std::mem::take(&mut toggle.mixed) {
                        toggle.control.set_checked(true);
                    }
                }
                outcome
            }
            Content::Slider(slider) => slider.handle(input, extent)?,
            Content::Field(field) => field.handle(&mut self.fonts.borrow_mut(), input)?,
            _ => Outcome::default(),
        };
        if radio {
            self.select_radio(target)?;
        }
        Ok(outcome)
    }
    pub fn effects(&mut self, target: NodeId, outcome: Outcome) -> Result {
        if outcome.repaint {
            self.dirty_visual_state(target)?;
        }
        if outcome.semantics {
            self.tree.mark_dirty(target, Dirty::SEMANTICS)?;
        }
        self.ime_dirty |= outcome.reset_ime;
        self.ime_reset |= outcome.reset_ime;
        if outcome.focus {
            self.set_focus(Some(target))?;
        }
        match outcome.capture {
            Some(Capture::Acquire(id)) => self.capture = Some((id, target)),
            Some(Capture::Release(id)) if self.capture == Some((id, target)) => self.capture = None,
            _ => {}
        }
        match outcome.clipboard {
            Some(Clipboard::Paste) => self.clipboard = Some(ClipboardRequest::Read),
            Some(request) => {
                let Content::Field(field) = &self.tree.get(target).unwrap().context.content else {
                    unreachable!("only editors request clipboard writes")
                };
                let text = field.editor().selected_text().to_owned();
                self.clipboard = Some(ClipboardRequest::Write(text));
                if request == Clipboard::Cut {
                    let outcome = self.control(target, Input::Paste(""))?;
                    self.effects(target, outcome)?;
                }
            }
            None => {}
        }
        if matches!(
            outcome.action,
            Some(Action::Activate | Action::Submit | Action::Change)
        ) {
            if let Some(handler) = self.callbacks.get(&target) {
                self.pending.push_back((target, handler.version));
            }
        }
        Ok(())
    }
    pub fn set_focus(&mut self, target: Option<NodeId>) -> Result {
        let change = self
            .focus
            .set(&self.tree, self.root, target, focus_policy)?;
        self.change_focus(change)
    }
    fn advance_focus(&mut self, direction: FocusDirection) -> Result {
        let change = self
            .focus
            .advance(&self.tree, self.root, direction, true, focus_policy)?;
        self.change_focus(change)
    }
    fn change_focus(&mut self, change: FocusChange) -> Result {
        self.reveal_target = change.current;
        if !change.changed() {
            return Ok(());
        }
        if let Some(previous) = change.previous {
            let outcome = self.control(previous, Input::Focus(false))?;
            self.effects(previous, outcome)?;
        }
        if let Some(current) = change.current {
            let outcome = self.control(current, Input::Focus(true))?;
            self.effects(current, outcome)?;
        }
        self.ime_reset = true;
        self.ime_dirty = true;
        self.tree.mark_dirty(self.root, Dirty::SEMANTICS)?;
        Ok(())
    }
    pub fn cancel_subtree(&mut self, root: NodeId) -> Result {
        if self
            .focus
            .current(&self.tree)
            .is_some_and(|id| self.contains(root, id))
        {
            self.set_focus(None)?;
        }
        if self.drag.is_some_and(|drag| self.contains(root, drag.node)) {
            self.drag = None;
        }
        if self.capture.is_some_and(|(_, id)| self.contains(root, id)) {
            let (_, id) = self.capture.take().unwrap();
            let outcome = self.control(id, Input::Cancel)?;
            self.effects(id, outcome)?;
        }
        if self.hover.is_some_and(|id| self.contains(root, id)) {
            let id = self.hover.take().unwrap();
            let outcome = self.control(
                id,
                Input::Pointer(PointerInput {
                    id: PointerId(0),
                    kind: PointerKind::Leave,
                    position: Point::default(),
                    inside: false,
                    modifiers: Modifiers::default(),
                }),
            )?;
            self.effects(id, outcome)?;
        }
        Ok(())
    }
    fn cancel_pointer(&mut self) -> Result {
        self.pointer = None;
        self.end_drag()?;
        if let Some((_, id)) = self.capture.take() {
            let outcome = self.control(id, Input::Cancel)?;
            self.effects(id, outcome)?;
        }
        if let Some(id) = self.hover.take() {
            let input = self.pointer_input(
                id,
                PointerId(0),
                PointerKind::Leave,
                Point::default(),
                Modifiers::default(),
            );
            self.dispatch(id, input)?;
            self.dirty_visual_state(id)?;
        }
        Ok(())
    }
    fn pointer_input(
        &self,
        target: NodeId,
        id: PointerId,
        kind: PointerKind,
        position: Point,
        modifiers: Modifiers,
    ) -> Input<'static> {
        let element = &self.tree.get(target).unwrap().context;
        let mut local = Point::new(
            position.x - element.bounds.origin.x,
            position.y - element.bounds.origin.y,
        );
        if matches!(element.content, Content::Field(_)) {
            let padding = element
                .padding
                .unwrap_or(element.theme_or(&self.theme).padding);
            local.x += element.scroll.x - padding;
            local.y += element.scroll.y - padding;
        }
        if matches!(element.content, Content::Slider(_)) {
            local.x -=
                aegle_widgets::slider_track(element.bounds.size, element.inset(&self.theme)).0;
        }
        Input::Pointer(PointerInput {
            id,
            kind,
            position: local,
            inside: element.bounds.contains(position)
                && element.clip.is_none_or(|clip| clip.contains(position)),
            modifiers,
        })
    }
    fn hit(&self, position: Point) -> Option<NodeId> {
        // A shown popup covers everything below it, including its padding.
        let popup = self.popup_at(position);
        if popup.is_none() {
            if let Some((id, _)) = self.scrollbar_at(position, None) {
                return Some(id);
            }
        }
        self.order.iter().rev().copied().find(|&id| {
            let element = &self.tree.get(id).unwrap().context;
            popup.is_none_or(|popup| self.contains(popup, id))
                && element.effective_visible
                && self.usable(id)
                && element.bounds.contains(position)
                && element.clip.is_none_or(|clip| clip.contains(position))
                && element.content.interactive()
        })
    }

    /// Geometry may move under a stationary pointer. Refresh hover without
    /// generating slider drag updates, text selections or application callbacks.
    pub fn rehit_pointer(&mut self) -> Result {
        if self.drag.is_some() {
            return Ok(());
        }
        if let Some((id, position)) = self.pointer {
            let hit = self.hit(position);
            self.update_hover(hit, id, position)?;
            self.sync_hover(id, position)?;
        }
        Ok(())
    }

    fn update_hover(&mut self, hit: Option<NodeId>, id: PointerId, position: Point) -> Result {
        if self.hover == hit {
            return Ok(());
        }
        if let Some(old) = self.hover.take() {
            let input =
                self.pointer_input(old, id, PointerKind::Leave, position, Modifiers::default());
            let outcome = self.control(old, input)?;
            self.effects(old, outcome)?;
            self.dirty_visual_state(old)?;
        }
        self.hover = hit;
        if let Some(hit) = hit {
            self.dirty_visual_state(hit)?;
        }
        Ok(())
    }

    fn sync_hover(&mut self, id: PointerId, position: Point) -> Result {
        if let Some(hit) = self.hover {
            let owns_pointer = self.capture.is_none_or(|capture| capture == (id, hit));
            if owns_pointer {
                let input =
                    self.pointer_input(hit, id, PointerKind::Move, position, Modifiers::default());
                let outcome = match &mut self.tree.get_mut(hit).unwrap().context.content {
                    Content::Slider(slider) => slider.update_hover(id, true),
                    Content::Button(..) | Content::Toggle(_) => self.control(hit, input)?,
                    _ => Outcome::default(),
                };
                self.effects(hit, outcome)?;
            }
        }
        Ok(())
    }
}
