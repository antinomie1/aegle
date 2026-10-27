//! Platform glyph rasterizers registered for system font handles.
//!
//! A [`FontSource::System`] run names a backend and an opaque face key; only
//! the crate that owns that backend knows how to turn the key into glyph
//! pixels. That crate registers one [`SystemRasterizer`] here, and the glyph
//! cache dispatches system fonts to it. There is no byte fallback: an
//! unregistered backend is [`GlyphError::UnsupportedGlyph`], because guessing
//! which font bytes a handle meant would pair glyph indices with other glyphs.
//!
//! The registry is process-wide and holds at most one rasterizer per backend
//! number. Registration happens on the UI thread when a platform text system is
//! created, before any paragraph using it is painted.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use aegle_scene::SystemFont;

use crate::{CacheLimits, GlyphError, Image, RasterOptions};

/// Platform rasterizer for fonts addressed by [`SystemFont`].
///
/// Implementations own whatever native font objects they cache and must not
/// keep a handle alive past the point where the backend invalidates it. The
/// trait is `Send` so the registry can live in a process-wide map.
pub trait SystemRasterizer: Send {
    /// Rasterizes one glyph of a registered platform font.
    ///
    /// `reserve` asks the calling cache to make room for `bytes` of incoming
    /// image data and returns its budget error; call it before allocating the
    /// pixels. An implementation returns [`GlyphError::UnsupportedGlyph`] for a
    /// glyph it cannot represent (color layers, SVG documents) instead of
    /// substituting another glyph.
    fn rasterize(
        &mut self,
        font: &SystemFont,
        glyph: u16,
        options: RasterOptions<'_>,
        limits: CacheLimits,
        reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
    ) -> Result<Image, GlyphError>;
}

fn registry() -> &'static Mutex<BTreeMap<u8, Box<dyn SystemRasterizer>>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<u8, Box<dyn SystemRasterizer>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Registers the rasterizer for `backend`, or returns `false` if it already has
/// one. The first registration wins so a later caller cannot redirect glyphs
/// another text system already refers to.
pub fn register_system_rasterizer(backend: u8, rasterizer: Box<dyn SystemRasterizer>) -> bool {
    let mut map = registry().lock().unwrap_or_else(PoisonError::into_inner);
    if map.contains_key(&backend) {
        return false;
    }
    map.insert(backend, rasterizer);
    true
}

/// Whether `backend` has a rasterizer, without rasterizing anything.
pub fn system_rasterizer_registered(backend: u8) -> bool {
    let map = registry().lock().unwrap_or_else(PoisonError::into_inner);
    map.contains_key(&backend)
}

/// Dispatches one system-font glyph to its backend rasterizer.
///
/// The registry lock is held for the call because the rasterizer is stateful
/// (`&mut self`); this path runs only on a glyph cache miss for a platform
/// font, never for shared font bytes.
pub(crate) fn rasterize(
    font: SystemFont,
    glyph: u16,
    options: RasterOptions<'_>,
    limits: CacheLimits,
    reserve: &mut dyn FnMut(usize) -> Result<(), GlyphError>,
) -> Result<Image, GlyphError> {
    let mut map = registry().lock().unwrap_or_else(PoisonError::into_inner);
    let rasterizer = map
        .get_mut(&font.backend)
        .ok_or(GlyphError::UnsupportedGlyph)?;
    rasterizer.rasterize(&font, glyph, options, limits, reserve)
}
