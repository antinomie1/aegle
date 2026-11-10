//! A custom title bar: the marked bar moves the window except where a usable
//! control lies over it, a grip resizes it, and the host sees the rectangles
//! only when they may have changed.

use std::{cell::RefCell, rc::Rc};

use aegle_ui::{
    CONTAINER, Control, ControlKind, Rect, ResizeEdge, Result, Size, TextSystem, Theme, Ui,
    WindowArea, container_style, control::PaintCx,
};

/// A control that takes pointer input, as a button does.
struct Pressable;

impl Control for Pressable {
    fn kind(&self) -> &'static ControlKind {
        &CONTAINER
    }
    fn interactive(&self) -> bool {
        true
    }
    fn paint(&mut self, _: &mut PaintCx<'_>) {}
}

#[test]
fn marked_controls_give_the_window_its_move_and_resize_areas() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(300.0, 200.0));
    let bar = ui.root().row();
    bar.set_height(32.0);
    let label = bar.column();
    label.set_width(200.0);
    label.set_height(32.0);
    let button = bar.add(|_, theme| Ok((Box::new(Pressable), container_style(theme, false))));
    button.set_width(40.0);
    button.set_height(32.0);
    let body = ui.root().row();
    body.set_height(168.0);
    let pressable = body.add(|_, theme| Ok((Box::new(Pressable), container_style(theme, false))));
    pressable.set_width(100.0);
    pressable.set_height(20.0);
    let grip = body.column();
    grip.set_width(16.0);
    grip.set_height(16.0);
    ui.refresh()?;

    // Nothing is marked, so a host has nothing to take.
    let mut areas = vec![(Rect::default(), WindowArea::Move)];
    assert!(!ui.take_window_areas(&mut areas));
    assert_eq!(areas.len(), 1);

    bar.set_window_area(WindowArea::Move);
    grip.set_window_area(WindowArea::Resize(ResizeEdge::BottomRight));
    assert_eq!(bar.window_area(), Some(WindowArea::Move));
    assert_eq!(label.window_area(), None);
    assert!(ui.take_window_areas(&mut areas));
    // The label inside the bar is part of it; the button over it is not,
    // and the control outside any area adds nothing.
    assert_eq!(
        areas,
        [
            (bar.bounds(), WindowArea::Move),
            (button.bounds(), WindowArea::Client),
            (grip.bounds(), WindowArea::Resize(ResizeEdge::BottomRight)),
        ]
    );
    let narrow = bar.bounds();
    assert!(narrow.size.width > 240.0 && narrow.size.height == 32.0);
    assert!(narrow.intersection(button.bounds()) == Some(button.bounds()));
    // Unchanged until something is repainted or marked.
    assert!(!ui.take_window_areas(&mut areas));
    ui.refresh()?;
    assert!(!ui.take_window_areas(&mut areas));

    // A disabled button no longer takes presses, so the bar covers it; a
    // hidden grip has no area; layout moves the rest.
    button.set_enabled(false);
    grip.set_visible(false);
    ui.resize(Size::new(400.0, 200.0));
    ui.refresh()?;
    assert!(ui.take_window_areas(&mut areas));
    assert_eq!(areas, [(bar.bounds(), WindowArea::Move)]);
    assert_eq!(bar.bounds().size.width, narrow.size.width + 100.0);

    // Any control can be excluded by hand, and marks go with their nodes.
    label.set_window_area(WindowArea::Client);
    assert!(ui.take_window_areas(&mut areas));
    assert_eq!(areas[1], (label.bounds(), WindowArea::Client));
    label.set_window_area(None);
    bar.remove();
    grip.remove();
    ui.refresh()?;
    assert!(ui.take_window_areas(&mut areas));
    assert!(areas.is_empty());
    ui.refresh()?;
    assert!(!ui.take_window_areas(&mut areas));
    Ok(())
}
