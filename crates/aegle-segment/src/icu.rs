//! The system ICU as the word segmenter: no dictionary is linked, the library
//! is loaded on first use.
//!
//! Linux distributions ship ICU as versioned shared objects whose C exports
//! carry the major version (`ubrk_open_78`); Windows ships `icu.dll` (since
//! Windows 10 1903) with the C API. Both are probed here, and a missing
//! library is remembered as "no segmenter" instead of being retried.
#![allow(unsafe_code)] // Calling C, and loading the library that provides it.
use std::{ffi::CString, os::raw::c_char, sync::OnceLock};

use crate::units;

/// `UBRK_WORD`, the break type that respects word boundaries and dictionaries.
const UBRK_WORD: i32 = 1;
/// `UBRK_DONE`, the value the iteration functions return at the end.
const UBRK_DONE: i32 = -1;
/// The oldest and newest ICU major versions whose exports are probed.
const VERSIONS: std::ops::RangeInclusive<u32> = 60..=100;

type Open = unsafe extern "C" fn(i32, *const c_char, *const u16, i32, *mut i32) -> *mut u8;
type Step = unsafe extern "C" fn(*mut u8) -> i32;
type Close = unsafe extern "C" fn(*mut u8);

/// A loaded system ICU.
struct Icu {
    // Dropped last: the function pointers below point into it.
    _library: libloading::Library,
    open: Open,
    first: Step,
    next: Step,
    close: Close,
}

static ICU: OnceLock<Option<Icu>> = OnceLock::new();

fn icu() -> Option<&'static Icu> {
    ICU.get_or_init(load).as_ref()
}

/// The segmenter entry point, or `None` when no system ICU was found.
pub(super) fn segments() -> Option<super::Segments> {
    icu()?;
    Some(segment)
}

fn segment(text: &str) -> Option<Vec<usize>> {
    let icu = icu()?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let length = i32::try_from(units.len()).ok()?;
    let locale = CString::new(crate::language::guess(text).unwrap_or("")).ok()?;
    let mut status: i32 = 0;
    // SAFETY: the opener only reads the locale and the UTF-16 text, whose
    // pointers and length are valid for the call, and writes one status code.
    let iterator = unsafe {
        (icu.open)(
            UBRK_WORD,
            locale.as_ptr(),
            units.as_ptr(),
            length,
            &mut status,
        )
    };
    if iterator.is_null() {
        return None;
    }
    if status > 0 {
        // SAFETY: opening returned an iterator, failed or not; ICU's dummy
        // iterator must still be closed.
        unsafe { (icu.close)(iterator) };
        return None;
    }
    let mut offsets = Vec::new();
    // SAFETY: the iterator was created above and is closed exactly once below.
    unsafe {
        let mut at = (icu.first)(iterator);
        while at != UBRK_DONE {
            offsets.push(at as usize);
            at = (icu.next)(iterator);
        }
        (icu.close)(iterator);
    }
    Some(units::to_bytes(text, &offsets))
}

/// The shared libraries to try, most specific first.
fn libraries() -> Vec<String> {
    #[cfg(target_os = "linux")]
    let names: Vec<String> = {
        // Break iteration lives in the i18n library; a merged `libicu` also
        // exists on some systems. Runtime systems usually have only the
        // versioned soname, so those are tried first.
        let mut names: Vec<String> = VERSIONS
            .rev()
            .map(|version| format!("libicui18n.so.{version}"))
            .collect();
        names.push("libicui18n.so".to_owned());
        names.push("libicu.so".to_owned());
        names
    };
    #[cfg(windows)]
    let names: Vec<String> = {
        // `icu.dll` (Windows 10 1903 and later) bundles both libraries;
        // earlier systems ship `icuin.dll` and `icuuc.dll` separately.
        ["icu.dll", "icuin.dll", "icuuc.dll"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    names
}

/// The symbol name suffixes to probe: Windows' system ICU exports the plain C
/// names, ICU's own builds carry the major version.
fn suffixes() -> Vec<String> {
    #[cfg(windows)]
    let mut suffixes = vec![String::new()];
    #[cfg(not(windows))]
    let mut suffixes: Vec<String> = Vec::new();
    suffixes.extend(VERSIONS.rev().map(|version| format!("_{version}")));
    suffixes
}

fn load() -> Option<Icu> {
    let suffixes = suffixes();
    for name in libraries() {
        // SAFETY: loading a shared library runs its initializers; ICU's are
        // designed for it, and the handle is kept alive by the `Icu` value.
        let Ok(library) = (unsafe { libloading::Library::new(&name) }) else {
            continue;
        };
        for suffix in &suffixes {
            if let Some(functions) = probe(&library, suffix) {
                let (open, first, next, close) = functions;
                return Some(Icu {
                    _library: library,
                    open,
                    first,
                    next,
                    close,
                });
            }
        }
    }
    None
}

/// Resolves the four functions of `library` under `suffix`.
fn probe(library: &libloading::Library, suffix: &str) -> Option<(Open, Step, Step, Close)> {
    // SAFETY: each symbol is resolved once and only called with the argument
    // types ICU documents; `Icu` keeps the library loaded for every call.
    unsafe {
        let open = *library
            .get::<Open>(format!("ubrk_open{suffix}\0").as_bytes())
            .ok()?;
        let first = *library
            .get::<Step>(format!("ubrk_first{suffix}\0").as_bytes())
            .ok()?;
        let next = *library
            .get::<Step>(format!("ubrk_next{suffix}\0").as_bytes())
            .ok()?;
        let close = *library
            .get::<Close>(format!("ubrk_close{suffix}\0").as_bytes())
            .ok()?;
        Some((open, first, next, close))
    }
}
