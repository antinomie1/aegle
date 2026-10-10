//! Decorators: drawing added around an existing control, driven by the
//! input and state that control sees, without changing its behavior.

use std::any::Any;

use aegle_controls::Input;
use aegle_core::{Dirty, NodeId};
use aegle_theme::VisualState;
use aegle_types::Point;

use crate::{Node, Result, control::PaintCx, state::State};

/// Draws under and over a control and observes the input it receives, for
/// effects such as press ripples on controls a library did not write.
///
/// The control handles every input first and keeps it: a decorator cannot
/// consume input or change the outcome. Both painters record in the node's
/// local coordinates with the same [`PaintCx`] as the control; an animated
/// decorator calls [`PaintCx::request_frame`] while it still moves, so an
/// idle one costs no frames. Decorators paint in the order they were added.
pub trait Decorator: Any {
    /// The control received `input`, with pointer positions in the node's
    /// local coordinates; `visual` is its state after handling it. Returns
    /// whether to repaint.
    fn input(&mut self, _input: &Input<'_>, _visual: VisualState) -> bool {
        false
    }
    /// Draws before the background and border.
    fn under(&mut self, _cx: &mut PaintCx<'_>) {}
    /// Draws after the content, under the focus outline.
    fn over(&mut self, _cx: &mut PaintCx<'_>) {}
}

impl Node {
    /// Adds a decorator after any this control already has; it lives as
    /// long as the control.
    pub fn decorate(&self, decorator: impl Decorator) {
        self.change(|state, id| {
            state
                .decorators
                .entry(id)
                .or_default()
                .push(Box::new(decorator));
            state.tree.mark_dirty(id, Dirty::PAINT)?;
            Ok(())
        })
    }
}

impl State {
    /// Shows a decorated control the input it just handled.
    pub(crate) fn observe(&mut self, id: NodeId, input: Input<'_>) -> Result {
        if self.decorators.is_empty() || !self.decorators.contains_key(&id) {
            return Ok(());
        }
        let visual = self.visual_state(id);
        let element = &self.tree.get(id).unwrap().context;
        // Controls see pointers relative to their content; decorators draw
        // relative to the node.
        let shift = element.control.content_offset(
            element.bounds.size,
            element.inset(&self.theme),
            element.scroll,
        );
        let input = match input {
            Input::Pointer(mut pointer) => {
                pointer.position =
                    Point::new(pointer.position.x - shift.x, pointer.position.y - shift.y);
                Input::Pointer(pointer)
            }
            input => input,
        };
        let mut repaint = false;
        for decorator in self.decorators.get_mut(&id).unwrap() {
            repaint |= decorator.input(&input, visual);
        }
        if repaint {
            self.tree.mark_dirty(id, Dirty::PAINT)?;
        }
        Ok(())
    }
}
