//! The Windows system text stack: DirectWrite for layout and GDI for glyph
//! outlines, both from the operating system.
//!
//! DirectWrite shapes, itemizes, applies bidi, breaks lines, falls back and
//! aligns exactly as the Windows shell does; Aegle keeps only the retained
//! paragraph, the caret geometry and the coverage masks it paints. Font runs
//! leave this module as backend-independent [`aegle_scene::SystemFont`] handles,
//! never as COM pointers, and `aegle-glyph` rasterizes them through GDI.

mod fonts;
mod layout;
mod metrics;
#[cfg(feature = "scene")]
mod paint;
mod runs;
mod sys;
mod system;

pub(crate) use layout::DirectWriteLayout;
pub(crate) use system::DirectWriteSystem;

/// Registers the GDI glyph rasterizer for this backend, once per process.
pub(crate) fn register_rasterizer() {
    let _ = aegle_glyph::gdi::register();
}
