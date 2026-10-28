//! The showcase drawn through the Linux system cairo library.
//!
//! The same window as `showcase`, with the same defaults — native platform,
//! system fonts, markup, motion, effects, COLRv1 and desktop services — and only
//! the renderer changed. Cairo rasterizes coverage through the distribution's
//! `libcairo.so.2`, loaded at run time; the shared compositor does the color work,
//! so pixel-aligned geometry comes out byte-identical to the tiny-skia backend.
//!
//! `cargo run -p aegle --release --features cairo --example showcase_cairo`
//!
//! A system without `libcairo.so.2` fails at startup with a clear error instead of
//! falling back to another rasterizer.
mod showcase {
    pub mod window;
}

use aegle::{app::RendererChoice, ui::Result};

fn main() -> Result {
    showcase::window::run(RendererChoice::named("cairo")?)
}
