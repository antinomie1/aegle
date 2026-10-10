//! Markup creates retained controls; ordinary Rust callbacks use typed IDs.
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/controls.aegle")?;
    view.clear.on_click(move |_| view.editor.set_text(""));
    let window = view.root.clone();
    view.dark.on_click(move |_| window.set_theme(Theme::dark()));
    view.close.on_click(move |_| view.root.close());
    app.run()
}
