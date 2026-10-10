//! A decorator adds a press ripple to a built-in button without changing its
//! behavior, and asks for frames only while the ripple moves.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use aegle_text::{Blob, GenericFamily};

use aegle_ui::{
    Color, Decorator, Modifiers, Point, PointerId, PointerKind, Result, Size, TextSystem, Theme,
    Ui, VisualState,
    control::{Input, PaintCx},
    scene::{Command, Rect, RoundedRect},
};
use aegle_widgets::Widgets;

const SPREAD: Duration = Duration::from_millis(300);
const INK: Color = Color::rgba(0, 0, 0, 31);

/// A circle growing from the press point, clipped to the control's shape.
#[derive(Default)]
struct Ripple {
    pressed: Option<Point>,
    started: Option<(Point, Instant)>,
    seen: Rc<Cell<Option<Point>>>,
}

impl Decorator for Ripple {
    fn input(&mut self, input: &Input<'_>, _: VisualState) -> bool {
        match input {
            Input::Pointer(pointer) if matches!(pointer.kind, PointerKind::Down { .. }) => {
                self.pressed = Some(pointer.position);
                self.seen.set(Some(pointer.position));
                true
            }
            _ => false,
        }
    }
    fn over(&mut self, cx: &mut PaintCx<'_>) {
        if let Some(at) = self.pressed.take() {
            self.started = Some((at, cx.time));
        }
        let Some((at, start)) = self.started else {
            return;
        };
        let progress = cx.time.duration_since(start).as_secs_f32() / SPREAD.as_secs_f32();
        if progress >= 1.0 {
            self.started = None;
            return;
        }
        let radius = cx.size.width.max(cx.size.height) * (0.1 + 0.9 * progress);
        let circle = Rect::new(at.x - radius, at.y - radius, radius * 2.0, radius * 2.0);
        cx.builder.push_clip(cx.shape);
        cx.builder.fill(RoundedRect::new(circle, radius), INK);
        cx.builder.pop();
        cx.request_frame();
    }
}

fn ink_fills(ui: &Ui) -> Result<usize> {
    let mut fills = 0;
    ui.visit_scenes(|visit| {
        if let aegle_ui::Visit::Scene { scene, .. } = visit {
            fills += scene
                .commands()
                .iter()
                .filter(|c| matches!(c, Command::Fill { color, .. } if *color == INK))
                .count();
        }
        Ok(())
    })?;
    Ok(fills)
}

#[test]
fn a_ripple_decorates_a_button_without_changing_it() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let button = ui.root().button("Ripple");
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    button.on_click(move |_| {
        counted.set(counted.get() + 1);
        Ok(())
    });
    let seen = Rc::new(Cell::new(None));
    button.decorate(Ripple {
        seen: seen.clone(),
        ..Default::default()
    });
    ui.resize(Size::new(200.0, 80.0));
    ui.refresh()?;
    assert!(!ui.wants_frames());

    let bounds = button.bounds();
    let at = Point::new(bounds.origin.x + 10.0, bounds.origin.y + 6.0);
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        at,
        Modifiers::default(),
    )?;
    // The decorator sees the press in the node's own coordinates.
    assert_eq!(seen.get(), Some(Point::new(10.0, 6.0)));
    ui.refresh()?;
    assert_eq!(ink_fills(&ui)?, 1);
    assert!(ui.wants_frames());

    // The button behaves as before: the release clicks it.
    ui.pointer(PointerId(1), PointerKind::Up, at, Modifiers::default())?;
    ui.dispatch_callbacks()?;
    assert_eq!(clicks.get(), 1);

    // An animation starting outside a frame starts at the refresh.
    let start = Instant::now();
    ui.run_frame(start + SPREAD / 2);
    ui.refresh()?;
    assert_eq!(ink_fills(&ui)?, 1);
    // Once the ripple has spread it draws nothing and stops asking for frames.
    ui.run_frame(start + SPREAD * 2);
    ui.refresh()?;
    assert_eq!(ink_fills(&ui)?, 0);
    assert!(!ui.wants_frames());
    Ok(())
}
