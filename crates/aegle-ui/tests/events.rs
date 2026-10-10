//! Frame callbacks run outside the UI borrow, in order, survive replacement
//! and stop with their control.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use aegle_ui::{Result, Size, TextSystem, Theme, Ui};

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(100.0, 100.0));
    Ok(ui)
}

#[test]
fn frame_callbacks_run_per_frame_until_stopped_or_removed() -> Result {
    let ui = ui()?;
    assert!(!ui.wants_frames());
    let log = Rc::new(RefCell::new(Vec::new()));
    let (a, b) = (ui.root().column(), ui.root().column());
    let start = Instant::now();
    for (node, name) in [(&a, "a"), (&b, "b")] {
        let log = log.clone();
        node.on_frame(move |node, now| {
            log.borrow_mut().push((name, now));
            // The UI is not borrowed: callbacks may change controls.
            node.set_width(10.0);
            true
        });
    }
    assert!(ui.wants_frames() && ui.refresh()?);
    ui.run_frame(start);
    ui.run_frame(start + Duration::from_millis(16));
    assert_eq!(
        *log.borrow(),
        [
            ("a", start),
            ("b", start),
            ("a", start + Duration::from_millis(16)),
            ("b", start + Duration::from_millis(16))
        ]
    );
    // Callbacks accumulate; one returning false stops, and one added during a
    // frame first runs on the next frame.
    let added = Rc::new(Cell::new(0));
    let count = added.clone();
    a.on_frame(move |node, _| {
        let count = count.clone();
        node.on_frame(move |_, _| {
            count.set(count.get() + 1);
            true
        });
        false
    });
    b.remove();
    log.borrow_mut().clear();
    ui.run_frame(start);
    ui.run_frame(start);
    assert_eq!((added.get(), log.borrow().len()), (1, 2));
    a.remove();
    let c = ui.root().column();
    c.on_frame(|_, _| false);
    ui.run_frame(start);
    assert!(!ui.wants_frames());
    Ok(())
}

#[test]
fn presses_count_into_double_and_triple_clicks() -> Result {
    use aegle_ui::{Modifiers, Point, PointerId, PointerKind};
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 200.0));
    let row = ui.root().row();
    row.set_width(100.0);
    row.set_height(40.0);
    let inner = row.column();
    inner.set_width(40.0);
    inner.set_height(40.0);
    ui.refresh()?;
    let hits = Rc::new(Cell::new(0));
    let count = hits.clone();
    // The inner column has no handler, so the row's runs for it.
    row.on_double_click(move |_| {
        count.set(count.get() + 1);
        Ok(())
    });
    let at = inner.bounds().origin;
    let press = |offset: f32, ms: u64| -> Result {
        let time = Instant::now() + Duration::from_millis(ms);
        let point = Point::new(at.x + 10.0 + offset, at.y + 10.0);
        let (id, none) = (PointerId(1), Modifiers::default());
        ui.pointer_at(id, PointerKind::Down { clicks: 1 }, point, none, time)?;
        ui.pointer_at(id, PointerKind::Up, point, none, time)?;
        ui.dispatch_callbacks()
    };
    press(0.0, 0)?;
    press(1.0, 200)?;
    assert_eq!(hits.get(), 1, "a second close press is a double click");
    press(1.0, 400)?;
    assert_eq!(hits.get(), 1, "a third is a triple click");
    press(1.0, 2000)?;
    press(20.0, 2100)?;
    assert_eq!(hits.get(), 1, "late or distant presses start over");
    ui.set_double_click(Duration::from_millis(50), 4.0);
    press(0.0, 3000)?;
    press(0.0, 3100)?;
    assert_eq!(hits.get(), 1, "the interval follows the system setting");
    Ok(())
}
