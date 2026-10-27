//! The DirectWrite font collection: application fonts plus the system set.
//!
//! `TextSystem::new()` must not see the machine's fonts, while
//! `TextSystem::system()` must. Application fonts are registered by bytes and
//! DirectWrite wants file references, so each registration is written once to a
//! temporary file that lives as long as the text system — the same trade-off
//! the Pango backend makes, and it keeps the bytes owned by the operating
//! system instead of by a pointer DirectWrite could outlive.
//!
//! Registering again rebuilds the collection from every file registered so far
//! (plus the system font set when system fonts are enabled), because a
//! DirectWrite collection is immutable once created.
#![allow(unsafe_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_PROPERTY_ID_FAMILY_NAME, IDWriteFactory3, IDWriteFontCollection, IDWriteFontFile,
    IDWriteFontSet, IDWriteFontSetBuilder1,
};
use windows::core::Interface;

use super::sys::{WideString, factory};
use crate::TextError;

/// One registered font file.
pub(crate) struct Registered {
    /// Temporary file holding the bytes; removed when the text system drops.
    pub path: PathBuf,
}

/// A collection and everything it refers to.
pub(crate) struct Built {
    /// Formats are created against this.
    pub collection: IDWriteFontCollection,
    /// Font files the collection refers to.
    pub files: Vec<IDWriteFontFile>,
    /// Family names in the collection, in DirectWrite's order.
    pub families: Vec<String>,
}

/// Builds the collection from `registered` plus the system fonts if asked.
///
/// Returns `None` when there is nothing to shape with: no registrations and no
/// system fonts. Callers report that as an empty font policy instead of
/// inventing a family that DirectWrite would silently substitute.
pub(crate) fn build(
    registered: &[Registered],
    system_fonts: bool,
) -> Result<Option<Built>, TextError> {
    if registered.is_empty() && !system_fonts {
        return Ok(None);
    }
    let factory = factory().ok_or(TextError::InvalidFont)?;
    let factory3: IDWriteFactory3 = factory.cast().map_err(|_| TextError::InvalidFont)?;
    // SAFETY: each file reference names a path that outlives the call, and all
    // objects are reference counted. The set builder copies what it needs
    // before the temporary files are used again.
    unsafe {
        let builder: IDWriteFontSetBuilder1 = factory3
            .CreateFontSetBuilder()
            .map_err(|_| TextError::InvalidFont)?
            .cast()
            .map_err(|_| TextError::InvalidFont)?;
        let mut files: Vec<IDWriteFontFile> = Vec::with_capacity(registered.len());
        for entry in registered {
            let path = WideString::new(&entry.path.to_string_lossy());
            let file = factory
                .CreateFontFileReference(path.as_pcwstr(), None)
                .map_err(|_| TextError::InvalidFont)?;
            builder
                .AddFontFile(&file)
                .map_err(|_| TextError::InvalidFont)?;
            files.push(file);
        }
        if system_fonts {
            let system = factory3
                .GetSystemFontSet()
                .map_err(|_| TextError::InvalidFont)?;
            builder
                .AddFontSet(&system)
                .map_err(|_| TextError::InvalidFont)?;
        }
        let set = builder
            .CreateFontSet()
            .map_err(|_| TextError::InvalidFont)?;
        let collection: IDWriteFontCollection = factory3
            .CreateFontCollectionFromFontSet(&set)
            .map_err(|_| TextError::InvalidFont)?
            .cast()
            .map_err(|_| TextError::InvalidFont)?;
        let families = family_names(&set)?;
        Ok(Some(Built {
            collection,
            files,
            families,
        }))
    }
}

/// Writes `data` to a fresh temporary file and returns its path.
///
/// The caller owns the file and must delete it; a failed write removes the
/// partial file before returning.
pub(crate) fn temp_file(data: &[u8]) -> Result<PathBuf, TextError> {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("aegle-font-{}-{id}.otf", std::process::id()));
    if std::fs::write(&path, data).is_err() {
        let _ = std::fs::remove_file(&path);
        return Err(TextError::InvalidFont);
    }
    Ok(path)
}

/// The distinct family names in a font set.
fn family_names(set: &IDWriteFontSet) -> Result<Vec<String>, TextError> {
    // SAFETY: the set is live; the returned string list is reference counted.
    let list = unsafe { set.GetPropertyValues(DWRITE_FONT_PROPERTY_ID_FAMILY_NAME) }
        .map_err(|_| TextError::InvalidFont)?;
    // SAFETY: the list is live for the loop; each string is copied out before
    // the next call.
    unsafe {
        let count = list.GetCount();
        let mut names: Vec<String> = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Ok(length) = list.GetStringLength(index) else {
                continue;
            };
            let mut buffer = vec![0u16; length as usize + 1];
            if list.GetString(index, &mut buffer).is_err() {
                continue;
            }
            let text = String::from_utf16_lossy(&buffer[..length as usize]);
            if !text.is_empty() && !names.contains(&text) {
                names.push(text);
            }
        }
        Ok(names)
    }
}
