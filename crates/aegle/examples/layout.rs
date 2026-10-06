//! Advanced layout from markup: alignment, wrapping, grid, stack and absolute placement.
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    aegle::ui!(&app, "examples/layout.aegle")?;
    app.run()
}
