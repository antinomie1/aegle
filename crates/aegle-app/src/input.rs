use crate::{
    Result, Ui, UiError,
    state::{Content, State, focus_policy},
};
use aegle_controls::{
    Action, Capture, Input, Key, KeyInput, Modifiers, Outcome, PointerId, PointerInput, PointerKind,
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
        let hit = state.hit(position);
        let target = state
            .capture
            .filter(|(pointer, _)| *pointer == id)
            .map(|(_, id)| id)
            .or(hit);
        if state.hover != hit {
            if let Some(old) = state.hover.take() {
                let input = state.pointer_input(old, id, PointerKind::Leave, position, modifiers);
                state.dispatch(old, input)?;
            }
            state.hover = hit;
        }
        if let Some(target) = target {
            let input = state.pointer_input(target, id, kind, position, modifiers);
            state.dispatch(target, input)?;
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
    /// Scrolls the editor under a point by a finite logical vertical displacement.
    pub fn scroll(&self, position: Point, amount: f32) -> Result {
        if ![position.x, position.y, amount]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(UiError::InvalidValue.into());
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.rebuild_order();
        if let Some(id) = state.hit(position) {
            let element = &mut state.tree.get_mut(id).unwrap().context;
            if matches!(element.content, Content::Field(_)) {
                element.scroll.y = (element.scroll.y + amount).max(0.0);
                state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
                state.ime_dirty = true;
            }
        }
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
        Ok(
            match &mut self.tree.get_mut(target).unwrap().context.content {
                Content::Button(button, _) => button.handle(input),
                Content::Field(field) => field.handle(&mut self.fonts.borrow_mut(), input)?,
                _ => Outcome::default(),
            },
        )
    }
    pub fn effects(&mut self, target: NodeId, outcome: Outcome) -> Result {
        if outcome.repaint {
            self.tree.mark_dirty(target, Dirty::PAINT)?;
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
        if matches!(outcome.action, Some(Action::Activate | Action::Submit)) {
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
            let padding = element.padding.unwrap_or(self.theme.padding);
            local.x += element.scroll.x - padding;
            local.y += element.scroll.y - padding;
        }
        Input::Pointer(PointerInput {
            id,
            kind,
            position: local,
            inside: element.bounds.contains(position),
            modifiers,
        })
    }
    fn hit(&self, position: Point) -> Option<NodeId> {
        self.order.iter().rev().copied().find(|&id| {
            let element = &self.tree.get(id).unwrap().context;
            element.effective_visible
                && self.usable(id)
                && element.bounds.contains(position)
                && matches!(element.content, Content::Button(..) | Content::Field(_))
        })
    }
}
