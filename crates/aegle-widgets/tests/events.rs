//! The window key handler sees keys before the focused control, knows when
//! a text editor has focus, and may consume or replace itself; control event
//! handlers accumulate and stay installed after an error.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use aegle_ui::{Key, KeyInput, Modifiers, Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::Widgets;

fn key(key: Key) -> KeyInput<'static> {
    KeyInput {
        key,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    }
}

fn space(ui: &Ui) -> Result {
    ui.key(key(Key::Character(' ')))?;
    ui.key(KeyInput {
        pressed: false,
        ..key(Key::Character(' '))
    })?;
    ui.dispatch_callbacks()
}

#[test]
fn window_key_handler_sees_keys_first_and_can_consume_them() -> Result {
    let ui = Rc::new(Ui::with_fonts(
        Rc::new(RefCell::new(TextSystem::new())),
        Theme::light(),
    )?);
    ui.resize(Size::new(200.0, 200.0))?;
    let toggle = ui.root().check_box("", false)?;
    let field = ui.root().text_field("")?;
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    ui.on_key(move |event| {
        log.borrow_mut()
            .push((event.key, event.time, event.editing));
        Ok(event.key == Key::Character(' '))
    })?;
    let at = Instant::now() - Duration::from_millis(30);
    toggle.focus()?;
    ui.refresh()?;
    // Consumed: the focused check box does not toggle.
    ui.key_at(key(Key::Character(' ')), at)?;
    space(&ui)?;
    assert!(!toggle.is_checked()?);
    assert_eq!(seen.borrow()[0], (Key::Character(' '), at, false));
    // Not consumed: Tab still moves focus, into the editor.
    ui.key(key(Key::Tab))?;
    ui.key(key(Key::Enter))?;
    assert!(field.visual_state()?.focused);
    let last = *seen.borrow().last().unwrap();
    assert_eq!((last.0, last.2), (Key::Enter, true));

    let inner = ui.clone();
    let replaced = Rc::new(Cell::new(false));
    let flag = replaced.clone();
    ui.on_key(move |_| {
        let flag = flag.clone();
        inner.on_key(move |_| {
            flag.set(true);
            Ok(true)
        })?;
        Ok(true)
    })?;
    ui.key(key(Key::Enter))?;
    ui.key(key(Key::Enter))?;
    assert!(replaced.get());
    ui.clear_on_key()?;
    ui.on_key(|_| Err("fails".into()))?;
    assert!(ui.key(key(Key::Enter)).is_err());
    // A failing handler stays installed and reports again until it is cleared.
    assert!(ui.key(key(Key::Enter)).is_err());
    ui.clear_on_key()?;
    toggle.focus()?;
    space(&ui)?;
    assert!(toggle.is_checked()?);
    Ok(())
}

#[test]
fn event_handlers_accumulate_and_survive_errors() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 200.0))?;
    let button = ui.root().button("Go")?;
    let log = Rc::new(RefCell::new(Vec::new()));
    let first = log.clone();
    button.on_click(move |_| {
        first.borrow_mut().push("first");
        Err("first fails".into())
    })?;
    let second = log.clone();
    button.on_click(move |button| {
        second.borrow_mut().push("second");
        // Added during dispatch: runs from the next click on.
        let late = second.clone();
        button.on_click(move |_| {
            late.borrow_mut().push("late");
            Ok(())
        })
    })?;
    button.activate()?;
    // Every handler of the event runs; the first error is reported.
    assert!(ui.dispatch_callbacks().is_err());
    assert_eq!(*log.borrow(), ["first", "second"]);
    // The failing handler stays installed.
    button.activate()?;
    assert!(ui.dispatch_callbacks().is_err());
    assert_eq!(log.borrow()[2..], ["first", "second", "late"]);
    button.clear_on_click()?;
    button.activate()?;
    ui.dispatch_callbacks()?;
    assert_eq!(log.borrow().len(), 5);
    Ok(())
}
