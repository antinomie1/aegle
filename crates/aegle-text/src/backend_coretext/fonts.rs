//! CoreText font objects: registered application fonts and named/system faces.
//!
//! `TextSystem::new()` registers font bytes and must not see the machine's
//! fonts; `TextSystem::system()` must. Application fonts go through
//! `CTFontManagerRegisterGraphicsFont` on a `CGFont` built from the bytes, which
//! makes them available to normal descriptor matching for the life of the
//! process. System fonts are reached by family name through
//! `CTFontCreateWithName`, whose fallback behavior CoreText handles.
//!
//! Every font object is reference counted by `CFRetained`; nothing here keeps a
//! raw pointer past the call that produced it.
#![allow(unsafe_code)]

use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{CGDataProvider, CGFont};
use objc2_core_text::CTFont;

use super::sys::cf_string;
use crate::TextError;

/// A registered application font, kept alive so its registration persists.
pub(crate) struct Registered {
    /// The graphics font the registration refers to.
    _font: CFRetained<CGFont>,
    /// Family names CoreText exposes for it.
    pub families: Vec<String>,
}

/// Registers the font bytes in `data` and returns the family names it provides.
///
/// CoreText reports the family through `CGFont`, which only exposes the
/// PostScript name; the OpenType `name` table is the authoritative source, so
/// `aegle-glyph`'s bounded parser reads it. A file with no readable family is
/// rejected rather than registered under a guessed name.
pub(crate) fn register(data: &[u8]) -> Result<Registered, TextError> {
    if data.is_empty() {
        return Err(TextError::InvalidFont);
    }
    let face = aegle_glyph::describe(data, 0).ok_or(TextError::InvalidFont)?;
    // `CFData` copies the bytes, so the provider and font own what they need.
    let cf_data = CFData::from_bytes(data);
    let provider = CGDataProvider::with_cf_data(Some(&cf_data)).ok_or(TextError::InvalidFont)?;
    let font = CGFont::with_data_provider(&provider).ok_or(TextError::InvalidFont)?;
    // SAFETY: the graphics font is live and owned; registration copies what it
    // needs and returns whether the font was accepted.
    let registered = unsafe {
        #[allow(deprecated)]
        objc2_core_text::CTFontManagerRegisterGraphicsFont(&font, std::ptr::null_mut())
    };
    if !registered {
        return Err(TextError::InvalidFont);
    }
    Ok(Registered {
        _font: font,
        families: vec![face.family],
    })
}

/// A font of `family` at `size` points, with `weight` and `slant` applied.
///
/// CoreText matches the closest installed face; when the family is unknown it
/// substitutes a default, which is CoreText's own fallback policy and the same
/// behavior the macOS shell shows. Returns `None` only when CoreText cannot
/// create any font at all.
pub(crate) fn font(
    family: &str,
    size: f64,
    weight: f32,
    slant: crate::Slant,
) -> Option<CFRetained<CTFont>> {
    let name = cf_string(family);
    let traits = super::sys::traits(weight, slant);
    let descriptor = super::sys::font_descriptor(&name, &traits);
    // SAFETY: the descriptor is live and owned; the returned font is reference
    // counted. A null matrix asks for the identity transform.
    Some(unsafe { CTFont::with_font_descriptor(&descriptor, size, std::ptr::null()) })
}
