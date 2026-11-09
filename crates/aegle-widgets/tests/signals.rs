//! Signals drive controls in Rust: bindings follow the signals they read,
//! end with their control, and two-way bindings write user changes back
//! without disturbing an edit in progress.
use aegle_text::GenericFamily;
use aegle_ui::{Key, KeyInput, Modifiers, Result, Signal, TextSystem, Theme, Ui};
use aegle_widgets::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())
}

#[test]
fn bindings_follow_what_they_read_and_end_with_their_control() -> Result {
    let ui = ui()?;
    let label = ui.root().text("");
    let count = Signal::new(0);
    let name = Signal::new(String::from("a"));
    let runs = Rc::new(Cell::new(0));
    // Dropped with the binding's closure.
    let canary = Rc::new(());
    let (tally, held, other) = (runs.clone(), canary.clone(), name.clone());
    count.bind(&label, move |label, n| {
        let _ = &held;
        tally.set(tally.get() + 1);
        // `name` is read only past 2, so only then does it rerun the binding.
        let text = match *n > 2 {
            true => format!("{n} {}", other.get()),
            false => n.to_string(),
        };
        label.set_text(&text);
    });
    assert_eq!((label.text(), runs.get()), ("0".into(), 1));
    name.set("b".into());
    assert_eq!(runs.get(), 1);
    count.set(3);
    assert_eq!((label.text(), runs.get()), ("3 b".into(), 2));
    name.set("c".into());
    assert_eq!((label.text(), runs.get()), ("3 c".into(), 3));
    count.set(3); // Equal: nothing reruns.
    count.update(|n| *n -= 2);
    assert_eq!((label.text(), runs.get()), ("1".into(), 4));
    name.set("d".into()); // Read on an earlier run only.
    assert_eq!(runs.get(), 4);

    // Change handlers run after the bindings, with the new value, and are
    // not rerun by their own changes.
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (log, shown, again) = (seen.clone(), label.clone(), count.clone());
    let watch = count.on_change(move |n| {
        log.borrow_mut().push((n, shown.text()));
        again.set(n + 10);
    });
    count.set(2);
    assert_eq!(*seen.borrow(), [(2, "2".to_string())]);
    assert_eq!((count.get(), label.text()), (12, "12 d".into()));
    watch.remove();
    count.set(5);
    assert_eq!(seen.borrow().len(), 1);

    // Removing the control drops its binding and everything it captured.
    let before = runs.get();
    assert_eq!(Rc::strong_count(&canary), 2);
    label.remove();
    assert_eq!(Rc::strong_count(&canary), 1);
    count.set(7);
    name.set("e".into());
    assert_eq!(runs.get(), before);
    Ok(())
}

#[test]
fn two_way_bindings_write_user_changes_back() -> Result {
    let ui = ui()?;
    let name = Signal::new(String::from("Ad"));
    let field = ui.root().text_field("");
    field.bind_text(&name);
    let echo = ui.root().text("");
    name.bind(&echo, |echo, name| echo.set_text(name));
    assert_eq!(field.text(), "Ad");

    // Typing in the middle: writing the text back must not set it again,
    // which would end the edit's caret and undo history.
    field.focus();
    field.select(aegle_text::Selection {
        anchor: 1,
        focus: 1,
    });
    let key = |c: char, modifiers| {
        ui.key(KeyInput {
            key: Key::Character(c),
            text: if modifiers == Modifiers::default() {
                "x"
            } else {
                ""
            },
            modifiers,
            pressed: true,
            repeat: false,
        })?;
        ui.dispatch_callbacks()
    };
    key('x', Modifiers::default())?;
    key('x', Modifiers::default())?;
    assert_eq!((name.get(), echo.text()), ("Axxd".into(), "Axxd".into()));
    let command = Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Modifiers::default()
    };
    key('z', command)?;
    assert_eq!(field.text(), "Ad");
    assert_eq!(name.get(), "Ad");
    name.set("Bo".into());
    assert_eq!(field.text(), "Bo");

    // The control clamps; the signal keeps what it was given.
    let volume = Signal::new(150.0);
    let slider = ui.root().slider(0.0, 10.0, 0.0);
    let binding = slider.bind_value(&volume);
    assert_eq!((slider.value(), volume.get()), (10.0, 150.0));
    slider.decrement();
    ui.dispatch_callbacks()?;
    assert!(volume.get() < 10.0);
    assert_eq!(slider.value(), volume.get());

    // Removing a two-way binding stops both directions.
    binding.remove();
    assert!(!binding.is_active());
    volume.set(3.0);
    assert_ne!(slider.value(), 3.0);
    slider.increment();
    ui.dispatch_callbacks()?;
    assert_eq!(volume.get(), 3.0);

    let dark = Signal::new(false);
    let check = ui.root().check_box("Dark", true);
    check.bind_checked(&dark);
    assert!(!check.is_checked());
    check.toggle();
    ui.dispatch_callbacks()?;
    assert!(dark.get());
    Ok(())
}
