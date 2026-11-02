//! Host handlers follow entry states: they run after bindings for changes
//! from markup and Rust, can be removed, and survive reloads with the
//! state's typed handles.

use aegle_loader::{Data, Program};
use aegle_ui::Result;
use std::{cell::RefCell, rc::Rc};

mod common;
use common::{click, names, ui};

fn program(source: &str) -> Result<Program> {
    let source = source.to_owned();
    Program::from_sources("main.aegle", &aegle_loader::Elements::new(), &mut |_| {
        Ok(source.clone())
    })
}

const COUNTER: &str = r#"Column {
    state count: int = 0
    state title: string = "volume"
    Text { text: title + " " + str(count) }
    Button { text: "up"; on clicked { count += 1 } }
}"#;

#[test]
fn change_handlers_see_settled_values_from_markup_and_rust() -> Result {
    let ui = ui()?;
    let view = program(COUNTER)?.build(&ui.root())?;
    let count = view.state::<i64>("count").unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let watch = count.on_change(move |value| log.borrow_mut().push(value));
    click(&ui, "up")?;
    count.set(5)?;
    // An unchanged value is no change.
    count.set(5)?;
    assert_eq!(*seen.borrow(), [1, 5]);
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["volume 5", "up"]);

    // A handler that changes its own state is not re-entered, and its error
    // reaches whoever changed the state.
    let clamp = count.clone();
    let clamped = count.on_change(move |value| -> Result {
        if value > 10 {
            clamp.set(10)?;
        }
        if value < 0 {
            return Err("negative".into());
        }
        Ok(())
    });
    count.set(12)?;
    assert_eq!(count.get(), 10);
    assert_eq!(*seen.borrow(), [1, 5, 12, 10]);
    assert_eq!(count.set(-1).unwrap_err().to_string(), "negative");
    clamped.remove();
    watch.remove();
    watch.remove();
    count.set(3)?;
    assert_eq!(seen.borrow().last(), Some(&-1));
    Ok(())
}

#[test]
fn handlers_and_handles_follow_their_state_through_reloads() -> Result {
    let ui = ui()?;
    let mut view = program(COUNTER)?.build(&ui.root())?;
    let count = view.state::<i64>("count").unwrap();
    let title = view.state::<String>("title").unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    count.on_change(move |value| log.borrow_mut().push(value));
    let titles = Rc::new(RefCell::new(0));
    let changed = titles.clone();
    title.on_change(move |_: String| *changed.borrow_mut() += 1);
    count.set(2)?;

    // `count` keeps its type; `title` becomes an int and is detached.
    let next = r#"Column {
    state title: int = 7
    state count: int = 0
    Text { text: "now " + str(count) + "/" + str(title) }
    Button { text: "up"; on clicked { count += 2 } }
}"#;
    view.reload(&program(next)?)?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["now 2/7", "up"]);
    click(&ui, "up")?;
    assert_eq!(*seen.borrow(), [2, 4]);
    count.set(9)?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["now 9/7", "up"]);
    assert_eq!(view.get("count"), Some(Data::Int(9)));
    // The detached handle changes nothing on screen and notifies nobody new.
    title.set("gone".into())?;
    assert_eq!(*titles.borrow(), 1);
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["now 9/7", "up"]);
    Ok(())
}
