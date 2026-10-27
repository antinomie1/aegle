//! DirectWrite COM objects and the process-wide factory.
//!
//! One `IDWriteFactory` serves every paragraph: creating it is expensive and
//! the COM objects it hands out are reference counted by the `windows` crate's
//! RAII wrappers, so no manual `Release` appears here. All COM calls are
//! `unsafe` FFI; each one's invariant is that the objects come from this
//! factory and stay alive for the call, which the wrappers guarantee.
#![allow(unsafe_code)]

use std::sync::OnceLock;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH, DWRITE_FONT_STYLE, DWRITE_FONT_WEIGHT,
    DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_JUSTIFIED,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteFactory, IDWriteFactory2,
};
use windows::core::{Interface, PCWSTR};

/// The shared DirectWrite factory, created on first use.
pub(crate) fn factory() -> Option<&'static IDWriteFactory> {
    static FACTORY: OnceLock<Option<IDWriteFactory>> = OnceLock::new();
    FACTORY
        .get_or_init(|| {
            // SAFETY: DWriteCreateFactory with the shared factory type is
            // documented to be callable from any thread and returns an owned
            // interface pointer that the wrapper releases.
            unsafe { windows::Win32::Graphics::DirectWrite::DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
                .ok()
        })
        .as_ref()
}

/// The factory as its version 2 interface, which adds font fallback.
pub(crate) fn factory2() -> Option<IDWriteFactory2> {
    factory()?.cast::<IDWriteFactory2>().ok()
}

/// Converts a CSS weight (1–1000) to a DirectWrite weight.
pub(crate) fn weight(value: f32) -> DWRITE_FONT_WEIGHT {
    DWRITE_FONT_WEIGHT(value.clamp(1.0, 1000.0).round() as i32)
}

/// Converts a CSS stretch ratio to the closest DirectWrite stretch step.
pub(crate) fn stretch(ratio: f32) -> DWRITE_FONT_STRETCH {
    // Each step is 100 units wide around normal (5); 1.0 maps to 5.
    let step = (ratio * 5.0).round().clamp(1.0, 9.0) as i32;
    DWRITE_FONT_STRETCH(step)
}

/// Converts an Aegle slant to a DirectWrite style.
pub(crate) fn style(slant: crate::Slant) -> DWRITE_FONT_STYLE {
    match slant {
        crate::Slant::Normal => DWRITE_FONT_STYLE(0),
        crate::Slant::Oblique(_) => DWRITE_FONT_STYLE(1),
        crate::Slant::Italic => DWRITE_FONT_STYLE(2),
    }
}

/// Converts an Aegle alignment to a DirectWrite text alignment.
pub(crate) fn alignment(value: crate::Alignment) -> DWRITE_TEXT_ALIGNMENT {
    match value {
        crate::Alignment::Start | crate::Alignment::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
        crate::Alignment::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
        crate::Alignment::Right | crate::Alignment::End => DWRITE_TEXT_ALIGNMENT_TRAILING,
        crate::Alignment::Justify => DWRITE_TEXT_ALIGNMENT_JUSTIFIED,
    }
}

/// Wrapping mode for a layout that wraps or does not.
pub(crate) fn wrapping(wrap: bool) -> DWRITE_WORD_WRAPPING {
    match wrap {
        true => DWRITE_WORD_WRAPPING_WRAP,
        false => DWRITE_WORD_WRAPPING_NO_WRAP,
    }
}

/// A NUL-terminated UTF-16 string for a COM call, living as long as the value.
pub(crate) struct WideString {
    units: Vec<u16>,
}

impl WideString {
    /// Encodes `text`, replacing interior NULs so the terminator is unambiguous.
    pub(crate) fn new(text: &str) -> Self {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        for unit in &mut units {
            if *unit == 0 {
                *unit = 0xfffd;
            }
        }
        units.push(0);
        Self { units }
    }

    /// The pointer to pass to a `PCWSTR` parameter.
    pub(crate) fn as_pcwstr(&self) -> PCWSTR {
        PCWSTR(self.units.as_ptr())
    }
}
