//! Shared retained editing, imperative callbacks and explicit theme selection.
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Aegle — retained controls")?;
    window.set_padding(16.0)?;
    window.text("Aegle / retained controls")?;
    window.text("One tree. Shared layout, input and CJK editing.")?;
    let field = window.text_area("Hello, 世界\n你好 / 日本語 / 한글")?;
    field.set_accessible_label("Text editor")?;
    field.set_grow(1.0)?;
    let row = window.row()?;
    let clear = row.button("Clear text")?;
    clear.on_click(move |_| field.set_text(""))?;
    let dark = row.button("Dark theme")?;
    let themed = window.clone();
    dark.on_click(move |_| themed.set_theme(Theme::dark()))?;
    let close = row.button("Close")?;
    close.on_click(move |_| window.close())?;
    app.run()
}
