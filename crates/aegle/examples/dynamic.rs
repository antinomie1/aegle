//! Compiled dynamic markup, plus a panel loaded and explicitly reloaded at run time.
use aegle::{loader::Program, prelude::*};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/dynamic.aegle")?;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/panel.aegle");
    let panel = Rc::new(RefCell::new(Program::load(path)?.build(&view.live)?));
    view.reload.on_click(move |_| {
        // A failed reload keeps the current panel; report it and keep running.
        let reloaded = Program::load(path).and_then(|program| panel.borrow_mut().reload(&program));
        if let Err(error) = reloaded {
            eprintln!("{error}");
        }
        Ok(())
    })?;
    app.run()
}
