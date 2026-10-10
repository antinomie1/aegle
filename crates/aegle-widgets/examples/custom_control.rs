//! A control with its own behavior, built on the `aegle-ui` engine's
//! `Control` trait the default controls use: a five-step rating that takes
//! focus, follows pointer hover and presses, answers arrow keys and the
//! semantic increment/decrement actions, paints with the shared appearance
//! (so skins, `Style`, themes and transitions apply) and reports changes to
//! accumulating handlers. A control library needs only `aegle-ui`. Runs
//! headless and prints what happened.
//!
//! `cargo run -p aegle-widgets --example custom_control`

use std::{cell::RefCell, rc::Rc};

use aegle_ui::{
    Accepts, Appearance, Container, Control, ControlKind, HandlerResult, Key, KeyInput, Modifiers,
    Point, PointerId, PointerKind, Result, Size, TextSystem, Theme, Ui, container_style,
    control::{Action, Frame, Input, InputCx, MeasureCx, Outcome, PaintCx},
    handle,
    scene::{Rect, RoundedRect},
};

const STEPS: u8 = 5;
const CELL: f32 = 18.0;
const GAP: f32 = 4.0;

/// The rating's kind: no box of its own, the focus outline and the indicator
/// color for chosen steps; it accepts interactive, pressed and indicator style.
static RATING: ControlKind = ControlKind {
    name: "Rating",
    skin: Appearance::base,
    accepts: Accepts::INTERACTIVE
        .with(Accepts::PRESSED)
        .with(Accepts::INDICATOR),
    container: false,
};

/// The behavior and state stored in the node.
struct RatingControl {
    value: u8,
    /// The step under the pointer, previewed while hovering.
    hover: Option<u8>,
}

impl RatingControl {
    /// The step under a local x position.
    fn step_at(x: f32) -> u8 {
        ((x / (CELL + GAP)).floor() as i32 + 1).clamp(1, i32::from(STEPS)) as u8
    }
    /// Sets the value from user input, reporting a change only if it moved.
    fn choose(&mut self, value: u8) -> Outcome {
        let changed = value != self.value;
        self.value = value;
        Outcome {
            handled: true,
            repaint: true,
            action: changed.then_some(Action::Change),
            ..Default::default()
        }
    }
}

impl Control for RatingControl {
    fn kind(&self) -> &'static ControlKind {
        &RATING
    }
    fn interactive(&self) -> bool {
        true
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Size {
        let steps = f32::from(STEPS);
        let width = steps * CELL + (steps - 1.0) * GAP + 2.0 * cx.padding;
        Size::new(width, CELL + 2.0 * cx.padding)
    }
    fn default_padding(&self, _: &Theme) -> f32 {
        2.0
    }
    fn content_offset(&self, _: Size, padding: f32, _: Point) -> Point {
        Point::new(padding, padding)
    }
    fn hover(&mut self, _: &mut InputCx<'_>, _: PointerId, input: Input<'_>) -> Outcome {
        let hover = match input {
            Input::Pointer(pointer) if pointer.inside => Some(Self::step_at(pointer.position.x)),
            _ => None,
        };
        let repaint = hover != self.hover;
        self.hover = hover;
        Outcome {
            repaint,
            ..Default::default()
        }
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Outcome {
        let (low, high) = (1, STEPS);
        match cx.logical(input) {
            Input::Pointer(pointer) if matches!(pointer.kind, PointerKind::Down { .. }) => {
                let outcome = self.choose(Self::step_at(pointer.position.x));
                Outcome {
                    focus: true,
                    ..outcome
                }
            }
            Input::Pointer(pointer) if pointer.kind == PointerKind::Leave => {
                self.hover = None;
                Outcome {
                    repaint: true,
                    ..Default::default()
                }
            }
            Input::Key(KeyInput {
                key, pressed: true, ..
            }) => match key {
                Key::Right | Key::Up => self.choose((self.value + 1).min(high)),
                Key::Left | Key::Down => self.choose(self.value.saturating_sub(1).max(low)),
                Key::Home => self.choose(low),
                Key::End => self.choose(high),
                _ => Outcome::default(),
            },
            Input::Increment => self.choose((self.value + 1).min(high)),
            Input::Decrement => self.choose(self.value.saturating_sub(1).max(low)),
            Input::Focus(_) => Outcome {
                repaint: true,
                ..Default::default()
            },
            _ => Outcome::default(),
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let shown = self.hover.unwrap_or(self.value);
        let look = cx.appearance;
        for step in 1..=STEPS {
            let x = cx.padding + f32::from(step - 1) * (CELL + GAP);
            let cell = RoundedRect::new(Rect::new(x, cx.padding, CELL, CELL), look.radius);
            let color = if step <= shown {
                look.indicator
            } else {
                look.border_color
            };
            cx.builder.fill(cell, color);
        }
    }
    #[cfg(feature = "accessibility")]
    fn action_input(
        &self,
        action: aegle_ui::accesskit::Action,
        _: Option<&aegle_ui::accesskit::ActionData>,
    ) -> Option<Input<'static>> {
        match action {
            aegle_ui::accesskit::Action::Increment => Some(Input::Increment),
            aegle_ui::accesskit::Action::Decrement => Some(Input::Decrement),
            _ => None,
        }
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.node.set_role(aegle_ui::accesskit::Role::Slider);
        cx.node.set_numeric_value(f64::from(self.value));
        cx.node.set_min_numeric_value(1.0);
        cx.node.set_max_numeric_value(f64::from(STEPS));
        cx.node.add_action(aegle_ui::accesskit::Action::Increment);
        cx.node.add_action(aegle_ui::accesskit::Action::Decrement);
    }
}

handle! {
    /// A five-step rating chosen by pointer or arrow keys.
    Rating(RatingControl): interactive, pressed, indicator
}

impl Rating {
    /// Appends a rating to `parent`.
    fn new(parent: &Container, value: u8) -> Self {
        let value = value.clamp(1, STEPS);
        let control = RatingControl { value, hover: None };
        Self(parent.add(|_, theme| Ok((Box::new(control), container_style(theme, false)))))
    }
    fn value(&self) -> u8 {
        self.read(|rating| rating.value)
    }
    /// Sets the value without calling the change handlers.
    fn set_value(&self, value: u8) {
        self.update(|rating| rating.value = value.clamp(1, STEPS));
    }
    /// Adds a handler run after the user changes the value.
    fn on_change<R: HandlerResult>(&self, mut callback: impl FnMut(Rating) -> R + 'static) {
        self.on_action(move |node| callback(Rating(node)).into_result())
    }
}

fn main() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    let rating = Rating::new(&ui.root(), 3);
    rating.set_indicator_color(aegle_ui::Color::rgb(230, 160, 0));
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = log.clone();
    rating.on_change(move |rating| {
        seen.borrow_mut().push(rating.value());
    });

    ui.resize(Size::new(200.0, 60.0));
    ui.refresh()?;
    // A press on the fifth cell focuses the control and chooses 5. Like a
    // real host, run the callbacks after each input batch: handlers read the
    // value current when they run.
    let bounds = rating.bounds();
    let fifth = Point::new(
        bounds.origin.x + bounds.size.width - 4.0,
        bounds.origin.y + 6.0,
    );
    let (pointer, none) = (PointerId(1), Modifiers::default());
    ui.pointer(pointer, PointerKind::Down { clicks: 1 }, fifth, none)?;
    ui.pointer(pointer, PointerKind::Up, fifth, none)?;
    ui.dispatch_callbacks()?;
    // Arrow keys step through the same behavior.
    for key in [Key::Left, Key::Left, Key::Home] {
        let input = KeyInput {
            key,
            text: "",
            modifiers: none,
            pressed: true,
            repeat: false,
        };
        ui.key(input)?;
        ui.dispatch_callbacks()?;
    }
    rating.set_value(4);
    ui.refresh()?;
    println!(
        "changes {:?}, now {}, focused {}",
        log.borrow(),
        rating.value(),
        rating.visual_state().focused
    );
    Ok(())
}
