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
//! Run on macOS 15 (x86_64): `tests/coretext.rs` covers generic families,
//! line baselines, CJK fallback and CoreGraphics rasterization. The
//! `document`, `editor` and `paragraph` tests still fail here (caret motion
//! across hard breaks, single-line reflow, missing-glyph counts and an
//! unfonted `TextSystem::new()`), so those behaviors remain unverified.

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
