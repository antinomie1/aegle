//! Nested retained viewports share clipping, focus, native input and semantics.
use aegle::{Point, prelude::*};

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/scrolling.aegle")?;
    view.bottom.on_click(move |_| view.last.ensure_visible());
    view.top
        .on_click(move |_| view.form.scroll_to(Point::default()));
    view.close.on_click(move |_| view.root.close());
    app.run()
}
