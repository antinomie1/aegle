//! Custom drawing, optionally with its own pointer, wheel and keyboard input.

use std::{any::Any, time::Instant};

use aegle_controls::{Capture, Input, Key, Modifiers, Outcome, PointerId, PointerKind};
use aegle_core::Dirty;
use aegle_layout::Style;
use aegle_scene::SceneBuilder;
use aegle_theme::{Appearance, ControlKind, Theme, VisualState};
use aegle_types::{Point, Size};
use aegle_ui::{
    Container, Control, Result,
    control::{InputCx, PaintCx},
    handle,
};

/// Records custom scene commands in local coordinates for a canvas of `Size`.
pub type Painter = dyn FnMut(&mut SceneBuilder, Size) -> Result;

handle!(
    Canvas,
    "A retained custom drawing whose painter re-records only after invalidation or resize."
);

/// Input received by an interactive canvas, in its local logical coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasEvent {
    /// The primary button went down; the canvas captures the pointer and
    /// takes focus until the matching release or cancel.
    Press {
        /// Local position.
        position: Point,
        /// Consecutive click count, from one.
        clicks: u8,
        /// Keyboard modifiers.
        modifiers: Modifiers,
        /// When the platform reported it.
        time: Instant,
    },
    /// The pointer moved: dragging while pressed (also outside the canvas)
    /// or hovering otherwise.
    Move {
        /// Local position.
        position: Point,
        /// Whether the primary button is held from a press on this canvas.
        pressed: bool,
        /// Keyboard modifiers.
        modifiers: Modifiers,
        /// When the platform reported it.
        time: Instant,
    },
    /// The primary button was released after a press on this canvas.
    Release {
        /// Local position.
        position: Point,
        /// Keyboard modifiers.
        modifiers: Modifiers,
        /// When the platform reported it.
        time: Instant,
    },
    /// The hovering pointer left the canvas.
    Leave,
    /// A press ended without release: capture or the window was lost.
    Cancel,
    /// Wheel or touchpad scrolling over the canvas, which it consumes.
    Wheel {
        /// Local position.
        position: Point,
        /// Scroll distance in logical pixels; positive moves content up/left.
        delta: Point,
        /// Keyboard modifiers, for example Ctrl to zoom.
        modifiers: Modifiers,
        /// When the platform reported it.
        time: Instant,
    },
    /// A key while the canvas has focus.
    Key {
        /// Logical key.
        key: Key,
        /// Translated text; empty for non-text keys.
        text: String,
        /// Keyboard modifiers.
        modifiers: Modifiers,
        /// Press versus release.
        pressed: bool,
        /// A held-key repeat.
        repeat: bool,
        /// When the platform reported it.
        time: Instant,
    },
    /// The canvas gained or lost keyboard focus.
    Focus(bool),
}

/// The control inside a [`Canvas`] node.
pub struct CanvasControl {
    painter: Box<Painter>,
    /// Takes focus and receives input, see [`Canvas::on_input`].
    interactive: bool,
    /// The pointer that pressed on this canvas.
    pressed: Option<PointerId>,
    /// Input waiting for the next callback dispatch.
    events: Vec<CanvasEvent>,
    /// A splitter grip's orientation (vertical) and first-pane share, for semantics.
    pub(crate) splitter: Option<(bool, f32)>,
}

/// A focus outline for interactive canvases, which have no frame of their own.
fn focusable(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        focus_width: 2.0,
        ..Appearance::new(theme, state)
    }
}

impl Canvas {
    /// Re-records the painter on the next refresh, for example after its data changed.
    pub fn invalidate(&self) -> Result {
        self.change(|state, id| {
            state.tree.mark_dirty(id, Dirty::PAINT)?;
            Ok(())
        })
    }
    /// Replaces the painter and re-records it on the next refresh.
    pub fn set_painter(
        &self,
        painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
    ) -> Result {
        self.change(|state, id| {
            state.control_as::<CanvasControl>(id).unwrap().painter = Box::new(painter);
            state.tree.mark_dirty(id, Dirty::PAINT)?;
            Ok(())
        })
    }
    /// Makes the canvas interactive: it joins Tab order, shows a focus
    /// outline, captures the pointer from a press until release, consumes
    /// wheel input over it and receives keys while focused. Events of one
    /// input batch are delivered in order after it, outside every UI borrow,
    /// like other callbacks. Replaces any previous input callback.
    pub fn on_input(
        &self,
        mut callback: impl FnMut(Canvas, CanvasEvent) -> Result + 'static,
    ) -> Result {
        self.change(|state, id| {
            state.control_as::<CanvasControl>(id).unwrap().interactive = true;
            state.decorations.entry(id).or_default().skin = Some(focusable);
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })?;
        self.on_action(move |node| {
            let events = node.change(|state, id| {
                Ok(std::mem::take(
                    &mut state.control_as::<CanvasControl>(id).unwrap().events,
                ))
            })?;
            for event in events {
                callback(Canvas(node.clone()), event)?;
            }
            Ok(())
        })
    }
    /// Stops input: the canvas leaves Tab order and drops queued events.
    pub fn clear_on_input(&self) -> Result {
        self.clear_on_action()?;
        self.change(|state, id| {
            let canvas = state.control_as::<CanvasControl>(id).unwrap();
            canvas.interactive = false;
            canvas.events.clear();
            if state.focus.current(&state.tree) == Some(id) {
                state.set_focus(None)?;
            }
            if let Some(decoration) = state.decorations.get_mut(&id) {
                decoration.skin = None;
                state.trim_decoration(id);
            }
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })
    }
}

impl CanvasControl {
    /// Queues an event; the first one of a batch schedules the callback.
    fn push(&mut self, event: CanvasEvent) -> Outcome {
        let first = self.events.is_empty();
        self.events.push(event);
        Outcome {
            handled: true,
            action: first.then_some(aegle_controls::Action::Change),
            ..Outcome::default()
        }
    }
}

impl Control for CanvasControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Container
    }
    fn interactive(&self) -> bool {
        self.interactive
    }
    fn drags(&self) -> bool {
        self.interactive
    }
    fn takes_wheel(&self) -> bool {
        self.interactive
    }
    fn hover(&mut self, cx: &mut InputCx<'_>, _: PointerId, input: Input<'_>) -> Result<Outcome> {
        match input {
            Input::Pointer(pointer) if self.pressed.is_none() => Ok(self.push(CanvasEvent::Move {
                position: pointer.position,
                pressed: false,
                modifiers: pointer.modifiers,
                time: cx.time,
            })),
            _ => Ok(Outcome::default()),
        }
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let time = cx.time;
        Ok(match input {
            Input::Pointer(p) => match p.kind {
                PointerKind::Down { clicks } => {
                    self.pressed = Some(p.id);
                    Outcome {
                        focus: true,
                        capture: Some(Capture::Acquire(p.id)),
                        ..self.push(CanvasEvent::Press {
                            position: p.position,
                            clicks,
                            modifiers: p.modifiers,
                            time,
                        })
                    }
                }
                PointerKind::Move if self.pressed.is_none_or(|id| id == p.id) => {
                    self.push(CanvasEvent::Move {
                        position: p.position,
                        pressed: self.pressed.is_some(),
                        modifiers: p.modifiers,
                        time,
                    })
                }
                PointerKind::Up if self.pressed == Some(p.id) => {
                    self.pressed = None;
                    Outcome {
                        capture: Some(Capture::Release(p.id)),
                        ..self.push(CanvasEvent::Release {
                            position: p.position,
                            modifiers: p.modifiers,
                            time,
                        })
                    }
                }
                PointerKind::Leave if self.pressed.is_none() => self.push(CanvasEvent::Leave),
                PointerKind::Cancel if self.pressed.take().is_some() => {
                    self.push(CanvasEvent::Cancel)
                }
                _ => Outcome::default(),
            },
            Input::Cancel if self.pressed.take().is_some() => self.push(CanvasEvent::Cancel),
            Input::Wheel {
                delta,
                position,
                modifiers,
            } => self.push(CanvasEvent::Wheel {
                position,
                delta,
                modifiers,
                time,
            }),
            Input::Key(key) => self.push(CanvasEvent::Key {
                key: key.key,
                text: key.text.to_owned(),
                modifiers: key.modifiers,
                pressed: key.pressed,
                repeat: key.repeat,
                time,
            }),
            Input::Focus(focused) => Outcome {
                repaint: true,
                ..self.push(CanvasEvent::Focus(focused))
            },
            _ => Outcome::default(),
        })
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        (self.painter)(cx.builder, cx.size)
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Orientation, Role};
        let Some((vertical, ratio)) = self.splitter else {
            cx.node.set_role(Role::Canvas);
            return;
        };
        // A focusable separator: the orientation is the divider line's.
        cx.node.set_role(Role::Splitter);
        cx.node.set_orientation(if vertical {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        });
        cx.node.set_numeric_value(f64::from(ratio) * 100.0);
        cx.node.set_min_numeric_value(0.0);
        cx.node.set_max_numeric_value(100.0);
    }
}

pub(crate) fn canvas(
    container: &Container,
    painter: impl FnMut(&mut SceneBuilder, Size) -> Result + 'static,
) -> Result<Canvas> {
    crate::add(container, |_, _| {
        Ok((
            Box::new(CanvasControl {
                painter: Box::new(painter),
                interactive: false,
                pressed: None,
                events: Vec::new(),
                splitter: None,
            }) as Box<dyn Control>,
            Style {
                flex_shrink: 0.0,
                ..Default::default()
            },
        ))
    })
    .map(Canvas)
}
