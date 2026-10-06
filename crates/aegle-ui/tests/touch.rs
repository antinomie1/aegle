//! Fingers: taps activate, drags on controls drag, drags on content pan and fling.
use aegle_ui::{Point, PointerId, Result, Size, TextSystem, Theme, TouchPhase, Ui};
use std::{cell::Cell, cell::RefCell, rc::Rc};

#[test]
fn taps_drags_and_pans_choose_by_what_is_under_the_finger() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let view = ui.root().scroll_view()?;
    view.set_size(Some(200.0), Some(100.0))?;
    view.set_padding(0.0)?;
    view.set_gap(0.0)?;
    let slider = view.slider(0.0, 100.0, 0.0)?;
    slider.set_size(Some(200.0), Some(30.0))?;
    let button = view.button("")?;
    button.set_size(Some(180.0), Some(40.0))?;
    for _ in 0..8 {
        view.button("")?.set_height(Some(40.0))?;
    }
    let taps = Rc::new(Cell::new(0));
    let counter = taps.clone();
    button.on_click(move |_| {
        counter.set(counter.get() + 1);
        Ok(())
    })?;
    ui.resize(Size::new(300.0, 200.0))?;
    ui.refresh()?;
    let finger = PointerId(1 << 40);
    let at = |x, y| Point::new(x, y);
    let touch = |phase, x, y, time| ui.touch(finger, phase, at(x, y), time);

    // A tap activates the button once and leaves no hover behind.
    touch(TouchPhase::Down, 50.0, 50.0, 0)?;
    touch(TouchPhase::Up, 50.0, 50.0, 40)?;
    ui.dispatch_callbacks()?;
    assert_eq!(taps.get(), 1);
    assert!(!button.visual_state()?.hovered);

    // A drag that starts on a button beyond the slop pans instead of activating.
    touch(TouchPhase::Down, 50.0, 80.0, 0)?;
    touch(TouchPhase::Move, 50.0, 70.0, 16)?; // Within the slop: still a press.
    assert_eq!(view.offset()?.y, 0.0);
    touch(TouchPhase::Move, 50.0, 40.0, 32)?;
    assert!(view.offset()?.y > 0.0, "the content follows the finger");
    let panned = view.offset()?.y;
    touch(TouchPhase::Move, 50.0, 30.0, 48)?;
    assert!(view.offset()?.y > panned);
    #[cfg(feature = "motion")]
    {
        touch(TouchPhase::Up, 50.0, 30.0, 64)?;
        assert!(ui.has_animations(), "a moving release keeps momentum");
        ui.stop_fling()?;
    }
    #[cfg(not(feature = "motion"))]
    touch(TouchPhase::Up, 50.0, 30.0, 64)?;
    ui.dispatch_callbacks()?;
    assert_eq!(taps.get(), 1, "a pan is not a tap");
    view.scroll_to(Point::new(0.0, 0.0))?;
    ui.refresh()?;

    // A finger on a slider drags the slider; nothing scrolls.
    touch(TouchPhase::Down, 20.0, 15.0, 0)?;
    touch(TouchPhase::Move, 120.0, 40.0, 16)?;
    assert!(slider.value()? > 40.0, "{}", slider.value()?);
    assert_eq!(view.offset()?.y, 0.0);
    touch(TouchPhase::Up, 120.0, 40.0, 32)?;

    // A cancelled contact ends without activating.
    touch(TouchPhase::Down, 50.0, 50.0, 0)?;
    touch(TouchPhase::Cancel, 50.0, 50.0, 8)?;
    ui.dispatch_callbacks()?;
    assert_eq!(taps.get(), 1);
    // Events for an unknown finger are ignored; coordinates must be finite.
    touch(TouchPhase::Move, 1.0, 1.0, 9)?;
    assert!(
        ui.touch(finger, TouchPhase::Down, at(f32::NAN, 0.0), 0)
            .is_err()
    );
    Ok(())
}
