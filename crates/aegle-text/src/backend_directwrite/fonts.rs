//! Building a DirectWrite font collection from in-memory application fonts.
//!
//! `TextSystem::new()` registers font bytes rather than files, so the collection
//! is built with `IDWriteFactory5`'s in-memory loader: each blob becomes one
//! `IDWriteFontFile` and the whole set becomes an `IDWriteFontCollection`, which
//! is what `CreateTextFormat` resolves family names against. The loader reads
//! the bytes through the pointer it is given, so this module also owns those
//! buffers for as long as the collection lives.
#![allow(unsafe_code)]

use std::sync::Arc;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_PROPERTY_ID_FAMILY_NAME, IDWriteFactory5, IDWriteFontCollection, IDWriteFontFile,
    IDWriteFontSetBuilder1,
};
use windows::core::Interface;

use super::sys::factory;
use crate::TextError;

/// An application font file the collection refers to.
pub(crate) struct Blob {
    /// Bytes the loader reads; must outlive the collection.
    pub data: Arc<[u8]>,
    /// Face index inside a collection file.
    pub index: u32,
}

/// A built collection together with everything that must outlive it.
pub(crate) struct Collection {
    /// The collection formats are created against.
    pub collection: IDWriteFontCollection,
    /// Font files the collection refers to.
    pub files: Vec<IDWriteFontFile>,
    /// Bytes those files point into.
    pub blobs: Vec<Arc<[u8]>>,
    /// Distinct family names in the collection, in DirectWrite's order.
    pub families: Vec<String>,
}

/// Builds a collection from `blobs`.
///
/// Fails when DirectWrite cannot parse any of the files, which is the only way
/// a caller learns that registering a font did not work.
pub(crate) fn build(blobs: &[Blob]) -> Result<Collection, TextError> {
    let factory = factory().ok_or(TextError::InvalidFont)?;
    let factory5: IDWriteFactory5 = factory.cast().map_err(|_| TextError::InvalidFont)?;
    // SAFETY: every buffer is kept in `blobs` and moved into the returned
    // `Collection`, so the pointers handed to the loader stay valid for as long
    // as the collection can read them. All objects are reference counted by the
    // wrappers.
    unsafe {
        let loader = factory5
            .CreateInMemoryFontFileLoader()
            .map_err(|_| TextError::InvalidFont)?;
        let builder: IDWriteFontSetBuilder1 = factory5
            .CreateFontSetBuilder()
            .map_err(|_| TextError::InvalidFont)?;
        let mut files = Vec::with_capacity(blobs.len());
        for blob in blobs {
            if blob.data.is_empty() {
                return Err(TextError::InvalidFont);
            }
            let file = loader
                .CreateInMemoryFontFileReference(
                    factory,
                    blob.data.as_ptr().cast(),
                    blob.data.len() as u32,
                    None,
                )
                .map_err(|_| TextError::InvalidFont)?;
            builder
                .AddFontFile(&file)
                .map_err(|_| TextError::InvalidFont)?;
            files.push(file);
        }
        let set = builder
            .CreateFontSet()
            .map_err(|_| TextError::InvalidFont)?;
        let collection: IDWriteFontCollection = factory5
            .CreateFontCollectionFromFontSet(&set)
            .map_err(|_| TextError::InvalidFont)?
            .cast()
            .map_err(|_| TextError::InvalidFont)?;
        let families = family_names(&set)?;
        let _ = &blobs[0].index;
        Ok(Collection {
            collection,
            files,
            blobs: blobs.iter().map(|blob| Arc::clone(&blob.data)).collect(),
            families,
        })
    }
}

/// The distinct family names in a font set.
fn family_names(
    set: &windows::Win32::Graphics::DirectWrite::IDWriteFontSet,
) -> Result<Vec<String>, TextError> {
    // SAFETY: the set is live; the returned list is reference counted.
    let list = unsafe { set.GetPropertyValues(DWRITE_FONT_PROPERTY_ID_FAMILY_NAME) }
        .map_err(|_| TextError::InvalidFont)?;
    // SAFETY: the list is live for the loop.
    unsafe {
        let count = list.GetCount();
        let mut names = Vec::with_capacity(count as usize);
        for index in 0..count {
            let length = match list.GetStringLength(index) {
                Ok(length) => length,
                Err(_) => continue,
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
