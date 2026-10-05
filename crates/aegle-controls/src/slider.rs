use crate::{
    Action, Button, Capture, Input, Key, KeyInput, Modifiers, Outcome, PointerId, PointerInput,
    PointerKind, Range, RangeError,
};
use aegle_types::Point;

/// Allocation-free horizontal slider behavior sharing button focus and capture.
///
/// The host translates pointer x into track coordinates and supplies track
/// extent. Matching drags continue outside the hit region. Cancellation retains
/// the last value. Arrow/semantic increments use one step, or 1% of a continuous
/// range; page keys use ten steps, or 10%. Home and End reach the endpoints.
#[derive(Clone, Debug)]
pub struct Slider {
    button: Button,
    drag: Option<PointerId>,
    range: Range,
}

impl Slider {
    /// Creates an enabled slider around an already validated range.
    pub fn new(range: Range) -> Self {
        Self {
            button: Button::new(),
            drag: None,
            range,
        }
    }
    /// Current validated range state.
    pub fn range(&self) -> &Range {
        &self.range
    }
    /// Changes range settings programmatically without emitting change actions.
    /// The host invalidates drawing and semantics when a range setter returns true.
    pub fn range_mut(&mut self) -> &mut Range {
        &mut self.range
    }
    /// Whether input may change the value.
    pub fn is_enabled(&self) -> bool {
        self.button.is_enabled()
    }
    /// Whether the host assigned keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.button.is_focused()
    }
    /// Whether the controlling pointer is inside the hit region.
    pub fn is_hovered(&self) -> bool {
        self.button.is_hovered()
    }
    /// Whether a drag is active, including outside the hit region.
    pub fn is_pressed(&self) -> bool {
        self.drag.is_some()
    }

    /// Refreshes hit membership after host geometry changes, without changing
    /// the value or capture. An active drag ignores other pointer identities.
    pub fn update_hover(&mut self, id: PointerId, inside: bool) -> Outcome {
        if self.drag.is_some_and(|drag| drag != id) {
            return Outcome::default();
        }
        self.button.handle(Input::Pointer(PointerInput {
            id,
            kind: PointerKind::Move,
            position: Point::default(),
            inside,
            modifiers: Modifiers::default(),
        }))
    }

    /// Disabling releases an active capture and retains the last numeric value.
    pub fn set_enabled(&mut self, enabled: bool) -> Outcome {
        if !enabled {
            self.drag = None;
        }
        self.button.set_enabled(enabled)
    }

    /// Applies physical or semantic input with finite nonnegative track extent.
    /// Zero extent disables pointer adjustment while preserving focus/capture
    /// and keyboard/semantic operation. Invalid geometry or SetValue input is
    /// rejected before changing state, even while disabled.
    pub fn handle(&mut self, input: Input<'_>, extent: f32) -> Result<Outcome, RangeError> {
        if !extent.is_finite() || extent < 0.0 {
            return Err(RangeError::InvalidExtent);
        }
        match input {
            Input::SetValue(value) if !value.is_finite() => return Err(RangeError::InvalidValue),
            Input::Pointer(pointer)
                if !pointer.position.x.is_finite() || !pointer.position.y.is_finite() =>
            {
                return Err(RangeError::InvalidExtent);
            }
            _ => {}
        }
        match input {
            Input::Focus(_) | Input::Cancel => Ok(self.shared(input)),
            Input::Pointer(pointer) => {
                if self.drag.is_some_and(|id| id != pointer.id) {
                    return Ok(Outcome::default());
                }
                let dragging = self.drag == Some(pointer.id);
                let mut result = self.shared(input);
                let adjust = matches!(result.capture, Some(Capture::Acquire(_)))
                    || (dragging && matches!(pointer.kind, PointerKind::Move | PointerKind::Up));
                if adjust {
                    result.handled = true;
                    if extent > 0.0 {
                        let fraction =
                            (f64::from(pointer.position.x) / f64::from(extent)).clamp(0.0, 1.0);
                        let value = if fraction == 1.0 {
                            self.range.max()
                        } else {
                            self.range.min() + (self.range.max() - self.range.min()) * fraction
                        };
                        let changed = self
                            .range
                            .set_value(value.clamp(self.range.min(), self.range.max()))?;
                        mark_change(&mut result, changed);
                    }
                }
                Ok(result)
            }
            Input::Key(key) if self.is_enabled() && self.is_focused() => self.key(key),
            Input::Increment if self.is_enabled() => self.nudge(true, false),
            Input::Decrement if self.is_enabled() => self.nudge(false, false),
            Input::SetValue(value) if self.is_enabled() => self.assign(value),
            _ => Ok(Outcome::default()),
        }
    }

    fn shared(&mut self, input: Input<'_>) -> Outcome {
        let mut result = self.button.handle(input);
        match result.capture {
            Some(Capture::Acquire(id)) => self.drag = Some(id),
            Some(Capture::Release(_)) => self.drag = None,
            None => {}
        }
        result.action = None;
        result
    }

    fn key(&mut self, key: KeyInput<'_>) -> Result<Outcome, RangeError> {
        if key.key == Key::Escape {
            return Ok(self.shared(Input::Key(key)));
        }
        if key.modifiers.control || key.modifiers.alt || key.modifiers.meta {
            return Ok(Outcome::default());
        }
        if !matches!(
            key.key,
            Key::Left
                | Key::Right
                | Key::Up
                | Key::Down
                | Key::PageUp
                | Key::PageDown
                | Key::Home
                | Key::End
        ) {
            return Ok(Outcome::default());
        }
        if !key.pressed {
            return Ok(Outcome {
                handled: true,
                ..Outcome::default()
            });
        }
        match key.key {
            Key::Home => self.assign(self.range.min()),
            Key::End => self.assign(self.range.max()),
            key => self.nudge(
                matches!(key, Key::Right | Key::Up | Key::PageUp),
                matches!(key, Key::PageUp | Key::PageDown),
            ),
        }
    }

    fn nudge(&mut self, up: bool, page: bool) -> Result<Outcome, RangeError> {
        let value = self.range.value();
        let step = self.range.step();
        let span = self.range.max() - self.range.min();
        let amount = if step == 0.0 {
            span / if page { 10.0 } else { 100.0 }
        } else {
            (step * if page { 10.0 } else { 1.0 }).min(span)
        };
        // The extra max endpoint descends to the adjacent last grid point.
        if !up && !page && step > 0.0 && value == self.range.max() {
            let remainder = span % step;
            if remainder > 0.0 {
                let result = self.assign(value - remainder)?;
                if result.action.is_some() {
                    return Ok(result);
                }
            }
        }
        let next = if up {
            let next = value + amount;
            if next == value { value.next_up() } else { next }
        } else {
            let next = value - amount;
            if next == value {
                value.next_down()
            } else {
                next
            }
        };
        self.assign(next.clamp(self.range.min(), self.range.max()))
    }

    fn assign(&mut self, value: f64) -> Result<Outcome, RangeError> {
        let mut result = Outcome {
            handled: true,
            ..Outcome::default()
        };
        mark_change(&mut result, self.range.set_value(value)?);
        Ok(result)
    }
}

fn mark_change(result: &mut Outcome, changed: bool) {
    if changed {
        result.repaint = true;
        result.semantics = true;
        result.action = Some(Action::Change);
    }
}
