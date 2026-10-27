//! DirectWrite objects shared by the paragraphs of one text system.
//!
//! A [`DirectWriteSystem`] owns the font collection text formats are created
//! against and the generic-family substitution table. `TextSystem::new()`
//! registers application fonts into a private collection built from their
//! bytes, so it never sees the machine's fonts; `TextSystem::system()` uses the
//! system collection. Fallback is DirectWrite's own system fallback attached to
//! each format, so a CJK character in a Latin family shapes with a CJK face.
#![allow(unsafe_code)]

use std::collections::BTreeMap;

use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory, IDWriteFontCollection, IDWriteFontFallback, IDWriteTextFormat,
    IDWriteTextFormat1,
};
use windows::core::{Interface, PCWSTR};

use super::fonts::{self, Blob};
use super::sys::{WideString, factory, factory2, stretch, style, weight};
use crate::style::Shaping;
use crate::{FontFamily, GenericFamily, TextError};

/// One registered application font file and the family names it provides.
struct Registered {
    id: u32,
    families: Vec<String>,
}

/// DirectWrite state shared by every paragraph of one text system.
pub(crate) struct DirectWriteSystem {
    collection: Option<IDWriteFontCollection>,
    fallback: Option<IDWriteFontFallback>,
    /// Keeps the collection's font files and their bytes alive.
    blobs: Vec<Blob>,
    files: Vec<windows::Win32::Graphics::DirectWrite::IDWriteFontFile>,
    registered: Vec<Registered>,
    generics: BTreeMap<GenericFamily, Vec<String>>,
    next_id: u32,
}

impl DirectWriteSystem {
    /// Creates the system, using the machine's fonts when `system_fonts` is set.
    pub fn new(system_fonts: bool) -> Option<Self> {
        let factory = factory()?;
        let collection = match system_fonts {
            true => system_collection(factory),
            false => None,
        };
        // SAFETY: the factory is live and returns a reference-counted fallback.
        let fallback =
            factory2().and_then(|factory| unsafe { factory.GetSystemFontFallback().ok() });
        Some(Self {
            collection,
            fallback,
            blobs: Vec::new(),
            files: Vec::new(),
            registered: Vec::new(),
            generics: BTreeMap::new(),
            next_id: 1,
        })
    }

    /// Resolves a CSS family list to the family a format is created with,
    /// replacing generic families that a registration stands in for.
    pub fn family(&self, css: &str) -> String {
        for token in css.split(',') {
            let name = token.trim().trim_matches(['"', '\'']);
            if name.is_empty() {
                continue;
            }
            let generic = match name.to_ascii_lowercase().as_str() {
                "serif" => Some(GenericFamily::Serif),
                "sans-serif" => Some(GenericFamily::SansSerif),
                "monospace" => Some(GenericFamily::Monospace),
                "cursive" => Some(GenericFamily::Cursive),
                "fantasy" => Some(GenericFamily::Fantasy),
                "system-ui" => Some(GenericFamily::SystemUi),
                "emoji" => Some(GenericFamily::Emoji),
                _ => None,
            };
            if let Some(first) = generic
                .and_then(|generic| self.generics.get(&generic))
                .and_then(|names| names.first())
            {
                return first.clone();
            }
            return name.to_owned();
        }
        "sans-serif".to_owned()
    }

    /// Creates the text format for one shaping key, with fallback attached.
    pub fn format(&self, shaping: &Shaping) -> Option<IDWriteTextFormat> {
        let factory = factory()?;
        let family = WideString::new(&self.family(&shaping.families));
        let locale = shaping.locale.as_deref().map(WideString::new);
        // SAFETY: both strings outlive the call and the collection is owned by
        // this system; the returned format is reference counted.
        let format = unsafe {
            factory
                .CreateTextFormat(
                    family.as_pcwstr(),
                    self.collection.as_ref(),
                    weight(shaping.weight),
                    style(shaping.slant),
                    stretch(shaping.width),
                    shaping.size,
                    locale
                        .as_ref()
                        .map_or(PCWSTR::null(), WideString::as_pcwstr),
                )
                .ok()?
        };
        if let (Some(fallback), Ok(format1)) = (&self.fallback, format.cast::<IDWriteTextFormat1>())
        {
            // SAFETY: both objects are live; the format keeps its own reference.
            let _ = unsafe { format1.SetFontFallback(fallback) };
        }
        Some(format)
    }

    /// Registers every face of one application font file.
    pub fn register_fonts(&mut self, data: &[u8]) -> Result<Vec<FontFamily>, TextError> {
        if data.len() < 4 {
            return Err(TextError::InvalidFont);
        }
        let tag = u32::from_be_bytes(data[0..4].try_into().unwrap());
        if tag != 0x0001_0000 && tag != 0x4f54_544f && tag != 0x7474_6366 && tag != 0x7472_7565 {
            return Err(TextError::InvalidFont);
        }
        let blob = Blob {
            data: std::sync::Arc::from(data),
            index: 0,
        };
        let built = fonts::build(std::slice::from_ref(&blob))?;
        let id = self.next_id;
        self.next_id += 1;
        self.registered.push(Registered {
            id,
            families: built.families.clone(),
        });
        for data in built.blobs {
            self.blobs.push(Blob { data, index: 0 });
        }
        self.files.extend(built.files);
        match self.collection {
            // A system collection cannot gain application fonts; registrations
            // then only map generic families onto installed names.
            Some(_) => {}
            None => self.collection = Some(built.collection),
        }
        Ok(vec![FontFamily(id)])
    }

    /// Makes registered `families` stand in for `generic`.
    pub fn set_generic_families(&mut self, generic: GenericFamily, families: &[FontFamily]) {
        let mut names = Vec::new();
        for family in families {
            if let Some(registered) = self.registered.iter().find(|r| r.id == family.0) {
                names.extend(registered.families.iter().cloned());
            }
        }
        match names.is_empty() {
            true => self.generics.remove(&generic),
            false => self.generics.insert(generic, names),
        };
    }

    /// Releases font files no format refers to; registered ids and names stay.
    pub fn trim(&mut self) {
        // Formats hold their own references to the collection, so dropping our
        // handles only frees what no live paragraph uses.
        self.files.clear();
        self.blobs.clear();
    }
}

impl std::fmt::Debug for DirectWriteSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectWriteSystem")
            .field("system_fonts", &self.collection.is_some())
            .field("fallback", &self.fallback.is_some())
            .field("registered", &self.registered.len())
            .finish()
    }
}

/// The machine's font collection, if the factory can produce one.
fn system_collection(factory: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    let mut collection: Option<IDWriteFontCollection> = None;
    // SAFETY: the out pointer is valid and the factory is alive; the returned
    // collection is reference counted.
    unsafe { factory.GetSystemFontCollection(&mut collection, false) }.ok()?;
    collection
}
