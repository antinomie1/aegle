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
    ui.resize(Size::new(100.0, 100.0))?;
    Ok(ui)
}

#[test]
fn frame_callbacks_run_per_frame_until_cleared_or_removed() -> Result {
    let ui = ui()?;
    assert!(!ui.wants_frames()?);
    let log = Rc::new(RefCell::new(Vec::new()));
    let (a, b) = (ui.root().column()?, ui.root().column()?);
    let start = Instant::now();
    for (node, name) in [(&a, "a"), (&b, "b")] {
        let log = log.clone();
        node.on_frame(move |node, now| {
            log.borrow_mut().push((name, now));
            // The UI is not borrowed: callbacks may change controls.
            node.set_width(10.0)
        })?;
    }
    assert!(ui.wants_frames()? && ui.refresh()?);
    ui.run_frame(start)?;
    ui.run_frame(start + Duration::from_millis(16))?;
    assert_eq!(
        *log.borrow(),
        [
            ("a", start),
            ("b", start),
            ("a", start + Duration::from_millis(16)),
            ("b", start + Duration::from_millis(16))
        ]
    );
    // Callbacks accumulate; one that clears and replaces them keeps only the
    // replacement, which first runs on the next frame.
    let replaced = Rc::new(Cell::new(0));
    let count = replaced.clone();
    a.on_frame(move |node, _| {
        let count = count.clone();
        node.clear_on_frame()?;
        node.on_frame(move |_, _| {
            count.set(count.get() + 1);
            Ok(())
        })
    })?;
    b.remove()?;
    log.borrow_mut().clear();
    ui.run_frame(start)?;
    ui.run_frame(start)?;
    assert_eq!((replaced.get(), log.borrow().len()), (1, 1));
    a.clear_on_frame()?;
    a.on_frame(|_, _| Err("stop".into()))?;
    assert!(ui.run_frame(start).is_err());
    assert!(!ui.wants_frames()?);
    Ok(())
}
