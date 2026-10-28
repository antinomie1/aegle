//! Every default control in one window, wired to a shared status line.
//!
//! `showcase vulkan`, `showcase wgpu` or `showcase cairo` selects a backend this
//! build has, and a trailing `rtl` lays the window out right to left.
mod showcase {
    pub mod window;
}

use aegle::ui::Result;

fn main() -> Result {
    let renderer = showcase::window::choose_renderer()?;
    showcase::window::run(renderer)
}
