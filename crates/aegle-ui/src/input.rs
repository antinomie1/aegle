// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.

use crate::{
    ClipboardRequest, Result, Ui, UiError,
    control::InputCx,
    state::{State, focus_policy},
};
use aegle_controls::{
    Action, Capture, Clipboard, Input, Key, KeyInput, Modifiers, Outcome, PointerId, PointerInput,
    PointerKind,
};
use aegle_core::{Dirty, EventPhase, FocusChange, FocusDirection, NodeId};
use aegle_text::ImeEdit;
use aegle_types::{Point, PointerButton};

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
                // The first control takes focus without showing it, as
                // Windows hides focus cues until the keyboard is used; the
                // first key press shows it.
                state.show_focus(false)?;
                state.advance_focus(FocusDirection::Forward)
            }
        } else {
            if let Some(id) = state.focus.current(&state.tree) {
                state.last_focus = Some(id);
            }
            state.set_focus(None)
        }
    }
    /// Delivers normalized keyboard input at the current time: first to the
    /// window key handler ([`Ui::on_key`]), then to the focused control;
    /// Tab/Shift+Tab traverse logical controls. See [`Ui::key_at`].
    pub fn key(&self, key: KeyInput<'_>) -> Result {
        self.key_at(key, std::time::Instant::now())
    }
    pub(crate) fn dispatch_key(&self, key: KeyInput<'_>) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if key.pressed {
            state.show_focus(true)?;
            for hook in state.hooks.clone() {
                if let Some(used) = hook.key
                    && used(&mut state, &key)?
                {
                    return Ok(());
                }
            }
            let shift_f10 = key.key == Key::Function(10) && key.modifiers.shift;
            if (key.key == Key::ContextMenu || shift_f10) && state.keyboard_context_menu() {
                return Ok(());
            }
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
    /// Delivers a primary pointer transition in logical window coordinates at
    /// the current time. See [`Ui::pointer_at`].
    pub fn pointer(
        &self,
        id: PointerId,
        kind: PointerKind,
        position: Point,
        modifiers: Modifiers,
    ) -> Result {
        self.pointer_at(id, kind, position, modifiers, std::time::Instant::now())
    }
    pub(crate) fn dispatch_pointer(
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
        if matches!(kind, PointerKind::Down { .. } | PointerKind::ButtonDown(_)) {
            state.show_focus(false)?;
            #[cfg(feature = "motion")]
            {
                state.motion.fling = None;
            }
            for hook in state.hooks.clone() {
                if let Some(press) = hook.press {
                    press(&mut state, position)?;
                }
            }
        }
        state.pointer =
            (!matches!(kind, PointerKind::Leave | PointerKind::Cancel)).then_some((id, position));
        let hit = state.pointer.and_then(|_| state.hit(position));
        let kind = match kind {
            PointerKind::Down { clicks } => PointerKind::Down {
                clicks: state.count_click(position, clicks),
            },
            kind => kind,
        };
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
        match (kind, state.node_at(position, false)) {
            (PointerKind::Down { clicks: 2 }, Some(node)) => state.double_click(node),
            (PointerKind::ButtonDown(PointerButton::Secondary), Some(node)) => {
                state.context_menu(node, position);
            }
            _ => {}
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
    /// See [`Self::wheel`] for wheel input with modifiers and a time.
    pub fn scroll_by(&self, position: Point, delta: Point) -> Result {
        self.wheel(
            position,
            delta,
            Modifiers::default(),
            std::time::Instant::now(),
        )
    }
    /// Delivers wheel input reported at `time`: a control under the point that
    /// takes wheel input (an interactive canvas) receives it first and may
    /// consume it; otherwise the nearest viewport or editor scrolls.
    pub fn wheel(
        &self,
        position: Point,
        delta: Point,
        modifiers: Modifiers,
        time: std::time::Instant,
    ) -> Result {
        if ![position.x, position.y, delta.x, delta.y]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(UiError::InvalidValue.into());
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        #[cfg(feature = "motion")]
        {
            state.motion.fling = None;
        }
        state.input_time = time;
        state.scroll_by_at(position, delta, Some(modifiers))?;
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
    /// Delivers `input` to `target` through the control, then applies the outcome; unusable targets ignore it.
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
    /// Delivers one input event to a node's control and returns its outcome,
    /// after running the work the control deferred.
    pub fn control(&mut self, target: NodeId, input: Input<'_>) -> Result<Outcome> {
        let fonts = self.fonts.clone();
        let theme = self.theme;
        let element = &mut self.tree.get_mut(target).unwrap().context;
        let (size, padding) = (element.bounds.size, element.inset(&theme));
        let mut deferred = Vec::new();
        let outcome = element.control.handle(
            &mut InputCx {
                fonts: &mut fonts.borrow_mut(),
                size,
                padding,
                time: self.input_time,
                deferred: &mut deferred,
                rtl: element.rtl,
            },
            input,
        )?;
        for work in deferred {
            work(self, target)?;
        }
        self.observe(target, input)?;
        Ok(outcome)
    }
    /// Applies a control outcome: invalidation, IME, focus, capture and clipboard requests.
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
                let Some(field) = self.tree.get(target).unwrap().context.control.editor() else {
                    return Err(UiError::WrongKind.into());
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
        ) && let Some(handler) = self.callbacks.get(&target)
        {
            self.pending.push_back((target, handler.version));
        }
        Ok(())
    }
    /// Shows or hides the focus indicator, repainting the focused control.
    fn show_focus(&mut self, visible: bool) -> Result {
        if self.focus_visible != visible {
            self.focus_visible = visible;
            if let Some(id) = self.focus.current(&self.tree) {
                self.dirty_visual_state(id)?;
            }
        }
        Ok(())
    }
    /// Moves logical focus to `target` (or clears it) under the shared enabled/visible policy.
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
    /// Releases focus, capture, hover and pointer routes that point into the subtree of `root`.
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
            self.hovered(None)?;
        }
        Ok(())
    }
    pub(crate) fn pointer_input(
        &self,
        target: NodeId,
        id: PointerId,
        kind: PointerKind,
        position: Point,
        modifiers: Modifiers,
    ) -> Input<'static> {
        let element = &self.tree.get(target).unwrap().context;
        let window = position;
        let position = self.untransform(target, position);
        let mut local = Point::new(
            position.x - element.bounds.origin.x,
            position.y - element.bounds.origin.y,
        );
        let shift = element.control.content_offset(
            element.bounds.size,
            element.inset(&self.theme),
            element.scroll,
        );
        local.x += shift.x;
        local.y += shift.y;
        Input::Pointer(PointerInput {
            id,
            kind,
            position: local,
            inside: element.bounds.contains(position)
                && element.clip.is_none_or(|clip| clip.contains(window)),
            modifiers,
        })
    }
    pub(crate) fn hit(&self, position: Point) -> Option<NodeId> {
        self.node_at(position, true)
    }
    /// The topmost visible, usable node under `position`, of any kind or
    /// only interactive controls.
    pub(crate) fn node_at(&self, position: Point, interactive: bool) -> Option<NodeId> {
        // A shown overlay covers everything below it, including its padding.
        let popup = self.overlay_at(position);
        if popup.is_none()
            && let Some((id, _)) = self.scrollbar_at(position, None)
        {
            return Some(id);
        }
        self.order.iter().rev().copied().find(|&id| {
            let element = &self.tree.get(id).unwrap().context;
            popup.is_none_or(|popup| self.contains(popup, id))
                && element.effective_visible
                && self.usable(id)
                && element.bounds.contains(self.untransform(id, position))
                && element.clip.is_none_or(|clip| clip.contains(position))
                && (!interactive || element.control.interactive())
        })
    }
}
