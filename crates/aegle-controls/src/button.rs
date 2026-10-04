use crate::{Action, Capture, Input, Key, Outcome, PointerId, PointerKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    Pointer(PointerId),
    Key(Key),
}

/// Retained button interaction without a visual style, label or allocation.
///
/// Pointer release activates only inside after a matching press. Space activates
/// on release; Enter activates once on press. Repeats never activate a button.
/// Semantic activation uses the same enabled check as physical input.
#[derive(Clone, Debug)]
pub struct Button {
    enabled: bool,
    focused: bool,
    hovered: bool,
    press: Option<Press>,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            enabled: true,
            focused: false,
            hovered: false,
            press: None,
        }
    }
}

impl Button {
    /// Creates an enabled button with no per-control allocation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether activation is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    /// Whether the host assigned keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }
    /// Whether a pointer is currently inside.
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }
    /// Whether to draw the pressed state; dragging outside disarms its visual.
    pub fn is_pressed(&self) -> bool {
        match self.press {
            Some(Press::Pointer(_)) => self.hovered,
            Some(Press::Key(_)) => true,
            None => false,
        }
    }

    /// Disabling cancels the press and releases capture without activation.
    /// The host must also move focus if its policy excludes disabled controls.
    pub fn set_enabled(&mut self, enabled: bool) -> Outcome {
        if self.enabled == enabled {
            return Outcome::default();
        }
        self.enabled = enabled;
        let mut result = if enabled {
            Outcome::default()
        } else {
            self.hovered = false;
            self.cancel()
        };
        result.repaint = true;
        result
    }

    /// Applies a target's default interaction. Custom listeners may prevent
    /// this call; pointer, keyboard and accessibility share the resulting action.
    pub fn handle(&mut self, input: Input<'_>) -> Outcome {
        let before = (self.focused, self.hovered, self.press);
        let mut result = Outcome::default();
        match input {
            Input::Focus(focused) => {
                self.focused = focused;
                if !focused {
                    result = self.cancel();
                }
            }
            Input::Cancel => {
                self.hovered = false;
                result = self.cancel();
            }
            Input::Activate if self.enabled => {
                result.handled = true;
                result.action = Some(Action::Activate);
            }
            Input::Pointer(pointer) if self.enabled => match pointer.kind {
                PointerKind::Move => self.hovered = pointer.inside,
                PointerKind::Leave => self.hovered = false,
                PointerKind::Down { .. } if pointer.inside && self.press.is_none() => {
                    self.hovered = true;
                    self.press = Some(Press::Pointer(pointer.id));
                    result.handled = true;
                    result.focus = true;
                    result.capture = Some(Capture::Acquire(pointer.id));
                }
                PointerKind::Up if self.press == Some(Press::Pointer(pointer.id)) => {
                    self.hovered = pointer.inside;
                    self.press = None;
                    result.handled = true;
                    result.capture = Some(Capture::Release(pointer.id));
                    result.action = pointer.inside.then_some(Action::Activate);
                }
                PointerKind::Cancel if self.press == Some(Press::Pointer(pointer.id)) => {
                    self.hovered = false;
                    result = self.cancel();
                }
                _ => {}
            },
            Input::Key(key) if self.enabled && self.focused => {
                if key.key == Key::Escape {
                    result = self.cancel();
                } else if matches!(key.key, Key::Enter | Key::Character(' ')) {
                    let modified = key.modifiers.control || key.modifiers.alt || key.modifiers.meta;
                    result.handled = !modified || self.press == Some(Press::Key(key.key));
                    if key.pressed && !key.repeat && !modified && self.press.is_none() {
                        self.press = Some(Press::Key(key.key));
                        result.handled = true;
                        if key.key == Key::Enter {
                            result.action = Some(Action::Activate);
                        }
                    } else if !key.pressed && self.press == Some(Press::Key(key.key)) {
                        self.press = None;
                        result.handled = true;
                        if key.key == Key::Character(' ') {
                            result.action = Some(Action::Activate);
                        }
                    }
                }
            }
            _ => {}
        }
        result.repaint |= before != (self.focused, self.hovered, self.press);
        result
    }

    fn cancel(&mut self) -> Outcome {
        let press = self.press.take();
        Outcome {
            handled: press.is_some(),
            repaint: press.is_some(),
            capture: match press {
                Some(Press::Pointer(id)) => Some(Capture::Release(id)),
                _ => None,
            },
            ..Outcome::default()
        }
    }
}
