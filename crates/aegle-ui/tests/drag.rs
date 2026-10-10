//! A drag crosses two drop targets: each gets one Enter followed by one
//! Leave or Drop, descendants defer to their nearest target, and removing a
//! target mid-drag ends it silently.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use aegle_ui::{DragData, DropEvent, Point, Result, Size, TextSystem, Theme, Ui};

#[test]
fn drags_reach_the_nearest_drop_target() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0))?;
    let row = ui.root().row()?;
    let (left, right) = (row.column()?, row.column()?);
    left.set_width(100.0)?;
    left.set_height(100.0)?;
    right.set_width(100.0)?;
    right.set_height(100.0)?;
    let inner = left.column()?;
    inner.set_width(50.0)?;
    inner.set_height(50.0)?;
    ui.refresh()?;

    let seen = Rc::new(RefCell::new(Vec::new()));
    for (name, zone) in [("left", &left), ("right", &right)] {
        let log = seen.clone();
        zone.on_drop(move |_, event| {
            log.borrow_mut().push((name, event));
            Ok(())
        })?;
    }
    // A second handler on the same target sees the same events.
    let count = Rc::new(RefCell::new(0));
    let counted = count.clone();
    right.on_drop(move |_, _| {
        *counted.borrow_mut() += 1;
        Ok(())
    })?;
    let take = || -> Result<Vec<(&str, DropEvent)>> {
        ui.dispatch_callbacks()?;
        Ok(std::mem::take(&mut *seen.borrow_mut()))
    };

    // Over the inner column, which has no handler of its own.
    assert!(ui.drag_motion(Point::new(10.0, 10.0))?);
    assert!(ui.drag_motion(Point::new(60.0, 60.0))?);
    assert_eq!(take()?, [("left", DropEvent::Enter)]);
    assert!(ui.drag_motion(Point::new(150.0, 10.0))?);
    assert_eq!(
        take()?,
        [("left", DropEvent::Leave), ("right", DropEvent::Enter)]
    );
    let data = DragData::Files(vec![PathBuf::from("/tmp/a b.txt")]);
    assert!(ui.drop_data(Point::new(150.0, 20.0), data.clone())?);
    let position = Point::new(150.0, 20.0);
    assert_eq!(take()?, [("right", DropEvent::Drop { data, position })]);
    assert_eq!(*count.borrow(), 2);

    // A drop on a target the drag had not entered still enters it first.
    assert!(ui.drop_data(Point::new(10.0, 10.0), DragData::Text("hi".into()))?);
    let events = take()?;
    assert_eq!(events[0], ("left", DropEvent::Enter));
    assert!(matches!(events[1], ("left", DropEvent::Drop { .. })));

    // Leaving the window and removing a target.
    ui.drag_motion(Point::new(10.0, 10.0))?;
    ui.drag_leave()?;
    assert_eq!(
        take()?,
        [("left", DropEvent::Enter), ("left", DropEvent::Leave)]
    );
    ui.drag_motion(Point::new(10.0, 10.0))?;
    left.remove()?;
    ui.refresh()?;
    assert!(take()?.is_empty(), "a removed target gets nothing");
    // A control's drag waits for the host once.
    right.start_drag(DragData::Text("moved".into()))?;
    assert_eq!(ui.take_drag()?, Some(DragData::Text("moved".into())));
    assert_eq!(ui.take_drag()?, None);
    Ok(())
}
