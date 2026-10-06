//! An interactive canvas captures drags, consumes wheel input before its scroll
//! view, takes focus and keys, and delivers events in order with their times.
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use aegle_ui::{
    Key, KeyInput, Modifiers, Point, PointerButton, PointerId, PointerKind, Result, Size,
    TextSystem, Theme, Ui,
};
use aegle_widgets::{CanvasEvent, Widgets};

#[test]
fn canvas_input_follows_capture_wheel_and_focus() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(300.0, 300.0))?;
    ui.root().set_padding(0.0)?;
    let view = ui.root().scroll_view()?;
    view.set_height(200.0)?;
    let canvas = view.canvas(|_, _| Ok(()))?;
    canvas.set_size(100.0, 400.0)?;
    ui.refresh()?;
    // Without input, the wheel scrolls the enclosing view.
    ui.scroll_by(Point::new(50.0, 50.0), Point::new(0.0, 30.0))?;
    assert_eq!(view.offset()?.y, 30.0);
    view.scroll_to(Point::default())?;

    let events = Rc::new(RefCell::new(Vec::new()));
    let log = events.clone();
    // The view's padding places the canvas at (4, 4); events are local to it.
    let origin = canvas.bounds()?.origin;
    let local = |x: f32, y: f32| Point::new(x - origin.x, y - origin.y);
    canvas.on_input(move |_, event| {
        log.borrow_mut().push(event);
        Ok(())
    })?;
    ui.refresh()?;
    let (id, mods) = (PointerId(1), Modifiers::default());
    let at = Instant::now() - Duration::from_millis(5);
    ui.pointer(id, PointerKind::Move, Point::new(10.0, 10.0), mods)?;
    ui.pointer_at(
        id,
        PointerKind::Down { clicks: 1 },
        Point::new(20.0, 30.0),
        mods,
        at,
    )?;
    // Captured: motion and release outside the canvas still arrive, in local coordinates.
    ui.pointer(id, PointerKind::Move, Point::new(250.0, 30.0), mods)?;
    ui.pointer(id, PointerKind::Up, Point::new(250.0, 40.0), mods)?;
    let ctrl = Modifiers {
        control: true,
        ..mods
    };
    // Focusing the canvas revealed it; the wheel itself must not scroll.
    ui.refresh()?;
    let revealed = view.offset()?;
    ui.wheel(Point::new(50.0, 50.0), Point::new(0.0, 30.0), ctrl, at)?;
    ui.key(KeyInput {
        key: Key::Character('n'),
        text: "n",
        modifiers: mods,
        pressed: true,
        repeat: false,
    })?;
    ui.dispatch_callbacks()?;
    let events = events.borrow();
    assert!(matches!(
        events[0],
        CanvasEvent::Move { pressed: false, .. }
    ));
    assert_eq!(
        events[1],
        CanvasEvent::Press {
            position: local(20.0, 30.0),
            clicks: 1,
            modifiers: mods,
            time: at
        }
    );
    assert_eq!(events[2], CanvasEvent::Focus(true));
    assert!(matches!(
        events[3],
        CanvasEvent::Move { position, pressed: true, .. } if position == local(250.0, 30.0)
    ));
    assert!(matches!(events[4], CanvasEvent::Release { .. }));
    assert!(matches!(
        events[5],
        CanvasEvent::Wheel { delta, modifiers, .. } if delta.y == 30.0 && modifiers.control
    ));
    assert!(matches!(&events[6], CanvasEvent::Key { text, .. } if text == "n"));
    assert_eq!(view.offset()?, revealed);
    assert!(canvas.visual_state()?.focused);
    assert!(canvas.appearance()?.focus_width > 0.0);
    drop(events);

    canvas.clear_on_input()?;
    assert!(!canvas.visual_state()?.focused);
    ui.scroll_by(Point::new(50.0, 50.0), Point::new(0.0, 30.0))?;
    assert_eq!(view.offset()?.y, revealed.y + 30.0);
    Ok(())
}

#[test]
fn other_buttons_reach_canvases_and_not_default_controls() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(300.0, 300.0))?;
    ui.root().set_padding(0.0)?;
    let slider = ui.root().slider(0.0, 10.0, 0.0)?;
    let canvas = ui.root().canvas(|_, _| Ok(()))?;
    canvas.set_size(100.0, 100.0)?;
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = events.clone();
    canvas.on_input(move |_, event| {
        log.borrow_mut().push(event);
        Ok(())
    })?;
    ui.refresh()?;
    let (id, mods) = (PointerId(1), Modifiers::default());
    let centre = |bounds: aegle_ui::scene::Rect| {
        Point::new(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        )
    };
    let right = PointerKind::ButtonDown(PointerButton::Secondary);
    let (thumb, inside) = (centre(slider.bounds()?), centre(canvas.bounds()?));
    ui.pointer(id, right, thumb, mods)?;
    ui.pointer(
        id,
        PointerKind::ButtonUp(PointerButton::Secondary),
        thumb,
        mods,
    )?;
    assert_eq!(slider.value()?, 0.0);
    assert!(!slider.visual_state()?.focused);

    // A middle drag keeps the pointer captured across a primary click and
    // ends with the last release; motion after it no longer arrives.
    let middle = PointerButton::Middle;
    let far = Point::new(290.0, 290.0);
    ui.pointer(id, PointerKind::ButtonDown(middle), inside, mods)?;
    ui.pointer(id, PointerKind::Move, far, mods)?;
    ui.pointer(id, PointerKind::Down { clicks: 1 }, far, mods)?;
    ui.pointer(id, PointerKind::Up, far, mods)?;
    ui.pointer(id, PointerKind::Move, far, mods)?;
    ui.pointer(id, PointerKind::ButtonUp(middle), far, mods)?;
    ui.pointer(id, PointerKind::Move, far, mods)?;
    // A lost window cancels a press of any button.
    ui.pointer(
        id,
        PointerKind::ButtonDown(PointerButton::Back),
        inside,
        mods,
    )?;
    ui.pointer_leave()?;
    ui.dispatch_callbacks()?;
    let kinds: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            CanvasEvent::ButtonPress { button, .. } => Some(format!("+{button:?}")),
            CanvasEvent::ButtonRelease { button, .. } => Some(format!("-{button:?}")),
            CanvasEvent::Press { .. } => Some("+primary".into()),
            CanvasEvent::Release { .. } => Some("-primary".into()),
            CanvasEvent::Move { pressed, .. } => Some(format!("move {pressed}")),
            CanvasEvent::Cancel => Some("cancel".into()),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "+Middle",
            "move false",
            "+primary",
            "-primary",
            "move false",
            "-Middle",
            "+Back",
            "cancel"
        ]
    );
    assert!(canvas.visual_state()?.focused);
    Ok(())
}
