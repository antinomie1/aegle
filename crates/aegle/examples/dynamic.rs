//! Compiled dynamic markup, plus a panel loaded and explicitly reloaded at run
//! time, a markup state linked to a Rust signal and one following a value
//! Rust derives from it.
use aegle::{loader::Program, prelude::*};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/dynamic.aegle")?;

    // The markup's `tasks` and this signal follow each other: the markup's
    // buttons change it, and so does Rust.
    let tasks = Signal::new(vec![String::from("Write markup"), "Run it".into()]);
    view.tasks.bind_signal(&tasks)?;
    let sorted = tasks.clone();
    view.sort
        .on_click(move |_| sorted.update(|tasks| tasks.sort()));
    // Markup has no `max`; Rust computes it and the summary shows it.
    view.longest.follow(&tasks.map(|tasks| {
        let lengths = tasks.iter().map(|task| task.chars().count() as i64);
        lengths.max().unwrap_or(0)
    }))?;

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/panel.aegle");
    let panel = Rc::new(RefCell::new(Program::load(path)?.build(&view.live)?));
    view.reload.on_click(move |_| {
        // A failed reload keeps the current panel; report it and keep running.
        let reloaded = Program::load(path).and_then(|program| panel.borrow_mut().reload(&program));
        if let Err(error) = reloaded {
            eprintln!("{error}");
        }
        Ok(())
    });
    app.run()
}
