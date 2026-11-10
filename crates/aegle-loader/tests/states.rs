//! Host handlers follow entry states: they run after bindings for changes
//! from markup and Rust, can be removed, and survive reloads with the
//! state's typed handles. A state linked to a Rust signal follows it both
//! ways; one following a derived value follows it one way and keeps it alive
//! with the interface. A component instance named by an id offers the same
//! handles to its own states.

use aegle_loader::{Data, Program};
use aegle_ui::{Derived, Result, Signal};
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

#[test]
fn a_state_linked_to_a_signal_follows_it_both_ways_through_reloads() -> Result {
    let ui = ui()?;
    let mut view = program(COUNTER)?.build(&ui.root())?;
    let count = view.state::<i64>("count").unwrap();
    let model = Signal::new(3_i64);
    let doubled = model.map(|n| n * 2);
    let link = count.bind_signal(&model)?;
    // The signal's value wins when they are linked.
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["volume 3", "up"]);

    // Markup changes reach Rust, and Rust changes reach markup, once each.
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (log, again) = (changes.clone(), changes.clone());
    model.on_change(move |n| log.borrow_mut().push(("signal", n)));
    count.on_change(move |n| again.borrow_mut().push(("state", n)));
    click(&ui, "up")?;
    assert_eq!((model.get(), doubled.get()), (4, 8));
    model.set(7);
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["volume 7", "up"]);
    // Each side heard each change once; the order follows registration.
    changes.borrow_mut().sort();
    assert_eq!(
        *changes.borrow(),
        [("signal", 4), ("signal", 7), ("state", 4), ("state", 7)]
    );

    // A markup-side failure of a signal change reaches the host.
    count.on_change(|n| -> Result {
        if n < 0 {
            return Err("negative".into());
        }
        Ok(())
    });
    model.set(-1);
    assert_eq!(count.get(), -1);
    assert_eq!(ui.dispatch_callbacks().unwrap_err().to_string(), "negative");
    assert!(aegle_ui::take_reaction_error().is_none());

    // The link follows the state into the reloaded interface.
    let next = r#"Column {
    state count: int = 0
    Text { text: "now " + str(count) }
    Button { text: "up"; on clicked { count += 10 } }
}"#;
    view.reload(&program(next)?)?;
    model.set(2);
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["now 2", "up"]);
    click(&ui, "up")?;
    assert_eq!(model.get(), 12);

    // Removing the link ends both directions.
    link.remove();
    model.set(5);
    click(&ui, "up")?;
    ui.refresh()?;
    assert_eq!(
        (names(&ui)?, model.get()),
        (vec!["now 22".to_string(), "up".into()], 5)
    );
    Ok(())
}

#[test]
fn a_state_follows_a_derived_value_one_way_while_the_interface_lives() -> Result {
    let ui = ui()?;
    let mut view = program(COUNTER)?.build(&ui.root())?;
    let count = view.state::<i64>("count").unwrap();
    let level = Signal::new(2_i64);
    // Dropped with the derived value.
    let canary = Rc::new(());
    let (held, source) = (canary.clone(), level.clone());
    // A temporary: the interface keeps it.
    let link = count.follow(&Derived::new(move || {
        let _ = &held;
        source.get() * 10
    }))?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["volume 20", "up"]);
    level.set(3);
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["volume 30", "up"]);

    // A markup assignment stands until the source next changes; it does not
    // reach the source.
    click(&ui, "up")?;
    assert_eq!((count.get(), level.get()), (31, 3));
    level.set(4);
    assert_eq!(count.get(), 40);

    // A failing assignment: now it is returned, later it reaches the host.
    count.on_change(|n| -> Result {
        if n < 0 {
            return Err("negative".into());
        }
        Ok(())
    });
    assert!(count.follow(&level.map(|n| -n)).is_err());
    level.set(-1);
    assert_eq!(ui.dispatch_callbacks().unwrap_err().to_string(), "negative");
    level.set(5);
    assert_eq!(count.get(), 50);

    // The link follows the state into a reloaded fragment, whose new root
    // takes the derived value over from the removed one.
    let next = r#"Column {
    state count: int = 0
    Text { text: "now " + str(count) }
    Button { text: "up"; on clicked { count += 1 } }
}"#;
    view.reload(&program(next)?)?;
    assert_eq!(Rc::strong_count(&canary), 2);
    level.set(6);
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["now 60", "up"]);

    // Removing the link stops it and releases the derived value.
    link.remove();
    assert_eq!(Rc::strong_count(&canary), 1);
    level.set(7);
    assert_eq!(count.get(), 60);

    // Removing the interface ends a link too.
    let (held, source) = (canary.clone(), level.clone());
    count.follow(&Derived::new(move || {
        let _ = &held;
        source.get()
    }))?;
    assert_eq!((count.get(), Rc::strong_count(&canary)), (7, 2));
    view.root().node().remove();
    assert_eq!(Rc::strong_count(&canary), 1);
    Ok(())
}

const TALLIES: &str = r#"component Tally(label: string) {
    state count: int = 0
    Button { text: label + " " + str(count); on clicked { count += 1 } }
}
Column {
    state names: list<string> = ["x"]
    Tally { id: first; label: "a" }
    for name in names { Tally { label: name } }
}"#;

#[test]
fn a_named_instance_offers_its_states_through_reloads() -> Result {
    let ui = ui()?;
    let mut view = program(TALLIES)?.build(&ui.root())?;
    assert!(view.instance("names").is_none());
    let first = view.instance("first").unwrap();
    let button = first.root().typed::<aegle_widgets::Button>().unwrap();
    assert_eq!(button.text(), "a 0");
    assert!(first.state::<String>("count").is_none());
    let count = first.state::<i64>("count").unwrap();

    // The instance's own state, from markup and from Rust; other instances
    // of the component keep theirs.
    click(&ui, "a 0")?;
    assert_eq!(count.get(), 1);
    count.set(5)?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["a 5", "x 0"]);
    let model = Signal::new(7_i64);
    count.bind_signal(&model)?;
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    count.on_change(move |n| log.borrow_mut().push(n));
    click(&ui, "a 7")?;
    assert_eq!(model.get(), 8);

    // A reload keeps the value and hands the handles, handler and link to
    // the instance with the same id and component.
    let next = TALLIES.replace("label + \" \" + str(count)", "label + \": \" + str(count)");
    view.reload(&program(&next)?)?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["a: 8", "x: 0"]);
    model.set(9);
    click(&ui, "a: 9")?;
    assert_eq!((count.get(), model.get()), (10, 10));
    assert_eq!(*seen.borrow(), [8, 9, 10]);

    // An id now naming another component's instance starts afresh; the old
    // handle keeps a detached value.
    let renamed = next.replace("Tally", "Counter");
    view.reload(&program(&renamed)?)?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["a: 0", "x: 0"]);
    count.set(3)?;
    ui.refresh()?;
    assert_eq!((count.get(), names(&ui)?[0].as_str()), (3, "a: 0"));
    let fresh = view
        .instance("first")
        .unwrap()
        .state::<i64>("count")
        .unwrap();
    fresh.set(4)?;
    ui.refresh()?;
    assert_eq!(names(&ui)?, ["a: 4", "x: 0"]);
    Ok(())
}
