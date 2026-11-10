//! Compiled dynamic markup, plus a panel loaded and explicitly reloaded at run
//! time, and a markup state linked to a Rust signal.
use aegle::{loader::Program, prelude::*};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/dynamic.aegle")?;

    // The markup's `tasks` and this signal follow each other: the markup's
    // buttons change it, and so does Rust.
    let tasks = Signal::new(vec![String::from("Write markup"), "Run it".into()]);
    view.tasks.bind_signal(&tasks)?;
    let rust = view.live.row();
    rust.set_gap(8.0, 8.0);
    rust.set_align_items(Align::Center);
    let sorted = tasks.clone();
    rust.button("Sort (Rust)")
        .on_click(move |_| sorted.update(|tasks| tasks.sort()));
    let longest = tasks.map(|tasks| tasks.iter().map(|task| task.chars().count()).max());
    longest.bind(&rust.text(""), |label, longest| {
        label.set_text(&format!(
            "Longest task: {} characters",
            longest.unwrap_or(0)
        ));
    });

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
