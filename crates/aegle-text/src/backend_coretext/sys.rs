//! CoreFoundation/CoreText helpers: strings, attribute dictionaries, font
//! descriptors and paragraph styles.
//!
//! Every CoreFoundation object here is owned by a `CFRetained` wrapper, so
//! reference counts balance without a manual `CFRelease`. The one place a raw
//! pointer appears is `CTParagraphStyleSetting::value`, which must point at a
//! live local for the duration of `CTParagraphStyleCreate`; that invariant is
//! stated at the call.
#![allow(unsafe_code)]

use objc2_core_foundation::{
    CFDictionary, CFMutableDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTParagraphStyle, CTParagraphStyleSetting, CTParagraphStyleSpecifier,
    CTTextAlignment, kCTFontAttributeName, kCTFontFamilyNameAttribute, kCTFontSlantTrait,
    kCTFontTraitsAttribute, kCTFontWeightTrait, kCTParagraphStyleAttributeName,
};

use crate::Alignment;

/// An immutable CoreFoundation string for a Rust `&str`.
pub(crate) fn cf_string(text: &str) -> CFRetained<CFString> {
    CFString::from_str(text)
}

/// A mutable dictionary keyed by `CFString` with `CFType` values, which is what
/// CoreText's attribute APIs take.
fn dictionary() -> CFRetained<CFMutableDictionary<CFString, CFType>> {
    CFMutableDictionary::with_capacity(3)
}

/// The traits dictionary for `weight` (CSS 1–1000) and `slant`.
///
/// CoreText expresses weight as a normalized `-1.0..1.0` value where 0.0 is
/// regular and 0.4 is bold; the CSS weight is mapped linearly across that range
/// so 400 lands on 0.0 and 700 on about 0.4. Slant is 0 upright and 1 italic.
pub(crate) fn traits(
    weight: f32,
    slant: crate::Slant,
) -> CFRetained<CFDictionary<CFString, CFType>> {
    let dictionary = dictionary();
    let normalized = ((weight - 400.0) / 750.0).clamp(-1.0, 1.0) as f64;
    let slant = match slant {
        crate::Slant::Normal => 0.0,
        crate::Slant::Italic | crate::Slant::Oblique(_) => 1.0,
    };
    let weight = CFNumber::new_f64(normalized);
    let slant = CFNumber::new_f64(slant);
    // SAFETY: both keys are static CoreFoundation strings and both values are
    // live CoreFoundation numbers; the dictionary retains them.
    unsafe {
        dictionary.set(kCTFontWeightTrait, &weight);
        dictionary.set(kCTFontSlantTrait, &slant);
    }
    // SAFETY: the dictionary holds `CFString` keys and `CFType` values.
    unsafe { CFRetained::cast_unchecked(dictionary) }
}

/// A font descriptor for `family` with the given traits.
///
/// When `family` names nothing installed, CoreText matches its closest
/// substitute; that is CoreText's own fallback and the behavior the macOS shell
/// has, so it is not an error here.
pub(crate) fn font_descriptor(
    family: &CFString,
    traits: &CFDictionary<CFString, CFType>,
) -> CFRetained<CTFontDescriptor> {
    let attributes = dictionary();
    // SAFETY: both keys are static CoreFoundation strings and both values are
    // live CoreFoundation objects.
    unsafe {
        attributes.set(kCTFontFamilyNameAttribute, family);
        attributes.set(kCTFontTraitsAttribute, traits);
    }
    // SAFETY: the dictionary is live and holds `CFString` keys with `CFType`
    // values, which is what the API expects; the descriptor is reference
    // counted.
    let attributes: CFRetained<CFDictionary> = unsafe { CFRetained::cast_unchecked(attributes) };
    unsafe { CTFontDescriptor::with_attributes(&attributes) }
}

/// A paragraph style with the given alignment and, when positive, a fixed line
/// height.
///
/// `line_height` becomes both the minimum and maximum line height, which pins
/// every line to that height. Without it CoreText's natural line height is used.
pub(crate) fn paragraph_style(
    alignment: Alignment,
    line_height: Option<f64>,
) -> CFRetained<CTParagraphStyle> {
    // The values must outlive the `CTParagraphStyleCreate` call, so they live in
    // locals here; `CTParagraphStyleSetting::value` points into them.
    let text_alignment: CTTextAlignment = match alignment {
        Alignment::Start | Alignment::Left => CTTextAlignment::Left,
        Alignment::Center => CTTextAlignment::Center,
        Alignment::Right | Alignment::End => CTTextAlignment::Right,
        Alignment::Justify => CTTextAlignment::Justified,
    };
    let height = line_height.unwrap_or(0.0);
    let mut settings: Vec<CTParagraphStyleSetting> = Vec::with_capacity(3);
    settings.push(CTParagraphStyleSetting {
        spec: CTParagraphStyleSpecifier::Alignment,
        valueSize: size_of::<CTTextAlignment>(),
        // SAFETY: `text_alignment` is a live local that outlives this call.
        value: std::ptr::NonNull::new(&text_alignment as *const _ as *mut core::ffi::c_void)
            .expect("stack local"),
    });
    if line_height.is_some() {
        for spec in [
            CTParagraphStyleSpecifier::MinimumLineHeight,
            CTParagraphStyleSpecifier::MaximumLineHeight,
        ] {
            settings.push(CTParagraphStyleSetting {
                spec,
                valueSize: size_of::<f64>(),
                // SAFETY: `height` is a live local for this call.
                value: std::ptr::NonNull::new(&height as *const _ as *mut core::ffi::c_void)
                    .expect("stack local"),
            });
        }
    }
    // SAFETY: every `value` points at a live local declared above, `valueSize`
    // matches the pointee, and the settings slice is valid for the call. The
    // returned style is an owned, immutable CoreFoundation object.
    unsafe { CTParagraphStyle::new(settings.as_ptr(), settings.len()) }
}

/// The attribute dictionary for one run: font and paragraph style.
pub(crate) fn text_attributes(
    font: &CTFont,
    style: &CTParagraphStyle,
) -> CFRetained<CFMutableDictionary<CFString, CFType>> {
    let attributes = dictionary();
    // SAFETY: both keys are static CoreFoundation strings and both values are
    // live CoreText objects.
    unsafe {
        attributes.set(kCTFontAttributeName, font);
        attributes.set(kCTParagraphStyleAttributeName, style);
    }
    attributes
}

/// Keeps the `CFType` import meaningful for the dictionary casts above.
const _: Option<fn() -> &'static CFType> = None;
