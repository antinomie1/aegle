//! Every default control in one window, wired to a shared status line.
//!
//! The window draws with the build's renderer and names it in the title:
//! `cargo run -p aegle --release --example showcase` uses tiny-skia, and
//! `--features wgpu` (or `vulkan`, `cairo`) builds the same window with
//! another one. A trailing `rtl` argument lays the window out right to left.
mod showcase {
    pub mod window;
}

use aegle::ui::Result;

fn main() -> Result {
    showcase::window::run()
}
