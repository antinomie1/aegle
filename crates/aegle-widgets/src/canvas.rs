//! Custom drawing, optionally with its own pointer, wheel and keyboard input.

use std::time::Instant;

use aegle_controls::{
    Capture, Input, Key, Modifiers, Outcome, PointerButton, PointerId, PointerKind,
};
use aegle_core::Dirty;
use aegle_scene::SceneBuilder;
use aegle_theme::ControlKind;
use aegle_types::{Point, Rect, Size};
use aegle_ui::LayoutStyle;
use aegle_ui::{
    Container, Control, HandlerResult,
    control::{InputCx, PaintCx},
    handle,
};

/// Records custom scene commands in local coordinates for a canvas of `Size`.
pub type Painter = dyn FnMut(&mut SceneBuilder, Size);

handle! {
    /// A retained custom drawing whose painter re-records only after invalidation or resize.
    pub Canvas(CanvasControl)
}

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
    /// Another button went down, such as the secondary button for a context
    /// action or the middle one to pan. The canvas captures the pointer until
    /// every button pressed on it is released; `Move` keeps `pressed` for the
    /// primary button only.
    ButtonPress {
        /// The button.
        button: PointerButton,
        /// Local position.
        position: Point,
        /// Keyboard modifiers.
        modifiers: Modifiers,
        /// When the platform reported it.
        time: Instant,
    },
    /// A button from a `ButtonPress` on this canvas was released.
    ButtonRelease {
        /// The button.
        button: PointerButton,
        /// Local position.
        position: Point,
        /// Keyboard modifiers.
        modifiers: Modifiers,
        /// When the platform reported it.
        time: Instant,
    },
    /// The hovering pointer left the canvas.
    Leave,
    /// Presses ended without release: capture or the window was lost.
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
    /// Clipboard text delivered while the canvas has focus, after
    /// [`Node::request_paste`](aegle_ui::Node::request_paste).
    Paste(String),
    /// One native IME transaction for a canvas published as a composition
    /// target with [`Canvas::set_ime`]; apply its fields in order.
    Ime(CanvasIme),
    /// Native text-input focus left: drop the composition without committing it.
    ImeCancelled,
}

/// One native IME transaction in UTF-8 byte offsets, owned so a canvas may
/// defer it to its own callback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanvasIme {
    /// Bytes to delete before the caret or composition.
    pub delete_before: usize,
    /// Bytes to delete after the caret or composition.
    pub delete_after: usize,
    /// Committed text replacing the deleted range; `Some("")` deletes.
    pub commit: Option<String>,
    /// Temporary text following the commit, empty when the composition ends.
    pub preedit: String,
    /// Fixed and active caret endpoints within `preedit`, in bytes.
    pub cursor: Option<(usize, usize)>,
}

/// The control inside a [`Canvas`] node.
pub struct CanvasControl {
    painter: Box<Painter>,
    /// Takes focus and receives input, see [`Canvas::set_input`].
    interactive: bool,
    /// The pointer that pressed on this canvas.
    pressed: Option<PointerId>,
    /// Buttons it holds: bit 0 is the primary one, then [`PointerButton`] in order.
    buttons: u8,
    /// Input waiting for the next callback dispatch.
    events: Vec<CanvasEvent>,
    /// Composition caret in local logical coordinates, see [`Canvas::set_ime`].
    ime: Option<Box<dyn FnMut(Size) -> Option<Rect>>>,
    /// A splitter grip's orientation (vertical) and first-pane share, for semantics.
    pub(crate) splitter: Option<(bool, f32)>,
}

impl Canvas {
    /// Re-records the painter on the next refresh, for example after its data changed.
    pub fn invalidate(&self) {
        self.change(|state, id| {
            state.invalidate(id, Dirty::PAINT)?;
            Ok(())
        })
    }
    /// Replaces the painter and re-records it on the next refresh.
    pub fn set_painter(&self, painter: impl FnMut(&mut SceneBuilder, Size) + 'static) {
        self.update(|canvas| canvas.painter = Box::new(painter))
    }
    /// Makes the canvas interactive: it joins Tab order, shows a focus
    /// outline, captures the pointer from a press until release, consumes
    /// wheel input over it and receives keys while focused. Events of one
    /// input batch are delivered in order after it, outside every UI borrow,
    /// like other callbacks. The input callback is the canvas's behavior, so
    /// like [`Self::set_painter`] it replaces any previous one.
    pub fn set_input<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(Canvas, CanvasEvent) -> R + 'static,
    ) {
        self.change(|state, id| {
            state.control_as::<CanvasControl>(id).unwrap().interactive = true;
            state.invalidate(id, Dirty::ALL)?;
            state.clear_actions(id);
            state.on_action(id, move |node| {
                let events = node.change(|state, id| {
                    Ok(std::mem::take(
                        &mut state.control_as::<CanvasControl>(id).unwrap().events,
                    ))
                });
                for event in events {
                    callback(Canvas(node.clone()), event).into_result()?;
                }
                Ok(())
            });
            Ok(())
        })
    }
    /// Makes an interactive canvas a native IME composition target: `caret`
    /// returns the composition caret rectangle in local logical coordinates
    /// (or `None` to end the session) for a canvas of the given size, and
    /// native transactions arrive as [`CanvasEvent::Ime`]. The engine publishes
    /// no surrounding excerpt, so the input method composes without it and only
    /// the caret anchors the candidate window. Call
    /// [`Node::request_ime_sync`](aegle_ui::Node::request_ime_sync) whenever the
    /// caret moves, and [`Self::set_input`] must be called too: a canvas that
    /// accepts no input takes no composition.
    pub fn set_ime(&self, caret: impl FnMut(Size) -> Option<Rect> + 'static) {
        self.change(|state, id| {
            state.control_as::<CanvasControl>(id).unwrap().ime = Some(Box::new(caret));
            state.invalidate(id, Dirty::PAINT)?;
            Ok(())
        })
    }
}

impl CanvasControl {
    /// Ends all presses without release events.
    fn cancel(&mut self) -> Outcome {
        self.buttons = 0;
        match self.pressed.take() {
            Some(_) => self.push(CanvasEvent::Cancel),
            None => Outcome::default(),
        }
    }

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
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
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
    fn hover(&mut self, cx: &mut InputCx<'_>, _: PointerId, input: Input<'_>) -> Outcome {
        match input {
            Input::Pointer(pointer) if self.pressed.is_none() => self.push(CanvasEvent::Move {
                position: pointer.position,
                pressed: false,
                modifiers: pointer.modifiers,
                time: cx.time,
            }),
            _ => Outcome::default(),
        }
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Outcome {
        let time = cx.time;
        match input {
            Input::Pointer(p) if self.pressed.is_some_and(|id| id != p.id) => Outcome::default(),
            Input::Pointer(p) => {
                let (position, modifiers) = (p.position, p.modifiers);
                let (bit, down) = match p.kind {
                    PointerKind::Down { .. } => (1, true),
                    PointerKind::Up => (1, false),
                    PointerKind::ButtonDown(button) => (2 << button as u8, true),
                    PointerKind::ButtonUp(button) => (2 << button as u8, false),
                    PointerKind::Move => {
                        return self.push(CanvasEvent::Move {
                            position,
                            pressed: self.buttons & 1 != 0,
                            modifiers,
                            time,
                        });
                    }
                    PointerKind::Leave if self.pressed.is_none() => {
                        return self.push(CanvasEvent::Leave);
                    }
                    PointerKind::Cancel => return self.cancel(),
                    PointerKind::Leave => return Outcome::default(),
                };
                if down == (self.buttons & bit != 0) {
                    return Outcome::default();
                }
                let first = self.buttons == 0;
                self.buttons ^= bit;
                self.pressed = (self.buttons != 0).then_some(p.id);
                let event = match p.kind {
                    PointerKind::Down { clicks } => CanvasEvent::Press {
                        position,
                        clicks,
                        modifiers,
                        time,
                    },
                    PointerKind::Up => CanvasEvent::Release {
                        position,
                        modifiers,
                        time,
                    },
                    PointerKind::ButtonDown(button) => CanvasEvent::ButtonPress {
                        button,
                        position,
                        modifiers,
                        time,
                    },
                    PointerKind::ButtonUp(button) => CanvasEvent::ButtonRelease {
                        button,
                        position,
                        modifiers,
                        time,
                    },
                    _ => unreachable!("other kinds returned above"),
                };
                let capture = match (first, self.buttons) {
                    (true, _) => Some(Capture::Acquire(p.id)),
                    (false, 0) => Some(Capture::Release(p.id)),
                    _ => None,
                };
                Outcome {
                    focus: first,
                    capture,
                    ..self.push(event)
                }
            }
            Input::Cancel => {
                let mut outcome = self.cancel();
                if self.ime.is_some() {
                    // Native text-input focus left: the composition ends
                    // without being committed.
                    let pushed = self.push(CanvasEvent::ImeCancelled);
                    outcome.action = outcome.action.or(pushed.action);
                    outcome.handled = true;
                }
                outcome
            }
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
            Input::Paste(text) if self.interactive => {
                self.push(CanvasEvent::Paste(text.to_owned()))
            }
            Input::Ime(edit) if self.ime.is_some() => self.push(CanvasEvent::Ime(CanvasIme {
                delete_before: edit.delete_before,
                delete_after: edit.delete_after,
                commit: edit.commit.map(str::to_owned),
                preedit: edit.preedit.to_owned(),
                cursor: edit.cursor.map(|selection| (selection.anchor, selection.focus)),
            })),
            _ => Outcome::default(),
        }
    }
    fn ime_caret(&mut self, size: Size) -> Option<Rect> {
        (self.ime.as_mut()?)(size)
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        (self.painter)(cx.builder, cx.size)
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::Role;
        let Some((vertical, ratio)) = self.splitter else {
            cx.set_role(Role::Canvas);
            return;
        };
        // A focusable separator: the orientation is the divider line's.
        cx.set_role(Role::Splitter);
        cx.set_vertical(!vertical);
        cx.set_numeric(f64::from(ratio) * 100.0, 0.0, 100.0);
    }
}

pub(crate) fn canvas(
    container: &Container,
    painter: impl FnMut(&mut SceneBuilder, Size) + 'static,
) -> Canvas {
    Canvas(crate::add(container, |_, _| {
        Ok((
            Box::new(CanvasControl {
                painter: Box::new(painter),
                interactive: false,
                pressed: None,
                buttons: 0,
                events: Vec::new(),
                ime: None,
                splitter: None,
            }) as Box<dyn Control>,
            {
                let mut style = LayoutStyle::default();
                style.set_shrink(0.0);
                style
            },
        ))
    }))
}
