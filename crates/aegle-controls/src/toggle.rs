use crate::{Action, Button, Input, Outcome};

/// A checked state sharing the button's keyboard, capture and activation rules.
///
/// Checkboxes and switches can use this same allocation-free behavior. The host
/// supplies their distinct visuals and semantic roles.
#[derive(Clone, Debug)]
pub struct Toggle {
    button: Button,
    checked: bool,
}

impl Toggle {
    /// Creates an enabled toggle with an explicit checked value.
    pub fn new(checked: bool) -> Self {
        Self {
            button: Button::new(),
            checked,
        }
    }
    /// The retained checked value.
    pub fn is_checked(&self) -> bool {
        self.checked
    }
    /// Whether changes through input are enabled.
    pub fn is_enabled(&self) -> bool {
        self.button.is_enabled()
    }
    /// Whether the host assigned keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.button.is_focused()
    }
    /// Whether a pointer is currently inside.
    pub fn is_hovered(&self) -> bool {
        self.button.is_hovered()
    }
    /// Whether the shared button behavior is visually pressed.
    pub fn is_pressed(&self) -> bool {
        self.button.is_pressed()
    }

    /// Sets state programmatically, invalidating changed presentation/semantics
    /// without producing an application change action.
    pub fn set_checked(&mut self, checked: bool) -> Outcome {
        let changed = self.checked != checked;
        self.checked = checked;
        Outcome {
            repaint: changed,
            semantics: changed,
            ..Outcome::default()
        }
    }
    /// Disabling cancels an outstanding press without changing the checked value.
    pub fn set_enabled(&mut self, enabled: bool) -> Outcome {
        self.button.set_enabled(enabled)
    }
    /// Applies shared activation rules and toggles only on actual activation.
    pub fn handle(&mut self, input: Input<'_>) -> Outcome {
        let mut result = self.button.handle(input);
        if result.action == Some(Action::Activate) {
            self.checked = !self.checked;
            result.action = Some(Action::Change);
            result.repaint = true;
            result.semantics = true;
        }
        result
    }
}
