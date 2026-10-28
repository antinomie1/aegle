//! The macOS system text stack: CoreText for shaping, line breaking and
//! metrics; CoreGraphics for glyph outlines.
//!
//! CoreText shapes, itemizes, applies bidi, breaks lines, falls back and aligns
//! exactly as the macOS shell does; Aegle keeps only the retained paragraph, the
//! caret geometry and the coverage masks it paints. Font runs leave this module
//! as backend-independent [`aegle_scene::SystemFont`] handles, never as native
//! pointers, and `aegle-glyph` rasterizes them through CoreGraphics.
//!
//! # Verification
//!
//! This module is **compile-checked only**: it is cross-compiled with
//! `cargo check`/`clippy` for `aarch64-apple-darwin` from Linux and has never
//! been run on a Mac. Behavior only a real system can confirm — CoreText's
//! fallback choices, line breaking and the caret geometry of complex scripts —
//! is unverified.

mod fonts;
mod layout;
mod metrics;
#[cfg(feature = "scene")]
mod paint;
mod runs;
mod sys;
mod system;

pub(crate) use layout::CoreTextLayout;
pub(crate) use system::CoreTextSystem;

/// Registers the CoreGraphics glyph rasterizer for this backend, once.
pub(crate) fn register_rasterizer() {
    let _ = aegle_glyph::cg::register();
}
