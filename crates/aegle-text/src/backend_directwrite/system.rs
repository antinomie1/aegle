//! DirectWrite state shared by the paragraphs of one text system.
//!
//! A [`DirectWriteSystem`] owns the font collection formats are created
//! against, the system fallback attached to each format, and the
//! generic-family substitution table. Application fonts are written to
//! temporary files on registration (see `fonts.rs`) and every registration
//! rebuilds the collection, because DirectWrite collections are immutable.
//!
//! `TextSystem::new()` therefore sees only registered fonts, and
//! `TextSystem::system()` adds the machine's font set. Fallback is
//! DirectWrite's own, so a CJK character in a Latin family shapes with a CJK
//! face instead of a missing-glyph box.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory, IDWriteFontCollection, IDWriteFontFallback, IDWriteFontFile, IDWriteTextFormat,
    IDWriteTextFormat1,
};
use windows::core::{Interface, PCWSTR};

use super::fonts::{self, Built, Registered};
use super::sys::{WideString, factory, factory2, stretch, style, weight};
use crate::style::Shaping;
use crate::{FontFamily, GenericFamily, TextError};

/// One registered application font and the family ids it answers to.
struct Registration {
    /// Ids handed to the caller; `FontFamily(id)`.
    ids: Vec<u32>,
    /// Family names the file provides, for generic-family mapping.
    families: Vec<String>,
}

/// DirectWrite state shared by every paragraph of one text system.
pub(crate) struct DirectWriteSystem {
    system_fonts: bool,
    collection: Option<IDWriteFontCollection>,
    fallback: Option<IDWriteFontFallback>,
    /// Font files the collection refers to.
    files: Vec<IDWriteFontFile>,
    /// Temporary files holding registered font bytes.
    temp: Vec<PathBuf>,
    /// Every registration so far, in order, for rebuilding the collection.
    registered: Vec<Registered>,
    /// Family ids per registration, for `set_generic_families`.
    registrations: Vec<Registration>,
    generics: BTreeMap<GenericFamily, Vec<String>>,
    next_id: u32,
}

impl DirectWriteSystem {
    /// Creates the system, using the machine's fonts when `system_fonts` is set.
    pub fn new(system_fonts: bool) -> Option<Self> {
        // The factory is the only hard requirement; an empty collection is a
        // valid state in which nothing shapes until fonts are registered.
        factory()?;
        let mut system = Self {
            system_fonts,
            collection: None,
            fallback: None,
            files: Vec::new(),
            temp: Vec::new(),
            registered: Vec::new(),
            registrations: Vec::new(),
            generics: BTreeMap::new(),
            next_id: 1,
        };
        // SAFETY: the factory is live; the fallback is reference counted.
        system.fallback =
            factory2().and_then(|factory| unsafe { factory.GetSystemFontFallback().ok() });
        system.rebuild().ok()?;
        Some(system)
    }

    /// Rebuilds the collection from every registration so far.
    fn rebuild(&mut self) -> Result<(), TextError> {
        let built = fonts::build(&self.registered, self.system_fonts)?;
        self.files.clear();
        match built {
            Some(Built {
                collection,
                files,
                families: _,
            }) => {
                self.collection = Some(collection);
                self.files = files;
            }
            None => self.collection = None,
        }
        Ok(())
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
        let path = fonts::temp_file(data)?;
        let entry = Registered { path: path.clone() };
        // Read the names by building a collection with only this file, so a
        // malformed font is rejected before the caller thinks it registered.
        let probe = fonts::build(std::slice::from_ref(&entry), false)?;
        let families = match probe {
            Some(built) => built.families,
            None => {
                let _ = std::fs::remove_file(&path);
                return Err(TextError::InvalidFont);
            }
        };
        if families.is_empty() {
            let _ = std::fs::remove_file(&path);
            return Err(TextError::InvalidFont);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.temp.push(path);
        self.registered.push(Registered {
            path: self.temp.last().expect("just pushed").clone(),
        });
        self.registrations.push(Registration {
            ids: vec![id],
            families,
        });
        self.rebuild()?;
        Ok(vec![FontFamily(id)])
    }

    /// Makes registered `families` stand in for `generic`.
    pub fn set_generic_families(&mut self, generic: GenericFamily, families: &[FontFamily]) {
        let mut names = Vec::new();
        for family in families {
            for registration in &self.registrations {
                if registration.ids.contains(&family.0) {
                    names.extend(registration.families.iter().cloned());
                }
            }
        }
        match names.is_empty() {
            true => self.generics.remove(&generic),
            false => self.generics.insert(generic, names),
        };
    }

    /// Releases font files no format refers to; registered ids and names stay.
    ///
    /// Live formats hold their own references to the collection and its files,
    /// so dropping these handles frees only what no paragraph uses.
    pub fn trim(&mut self) {
        self.files.clear();
    }
}

impl Drop for DirectWriteSystem {
    fn drop(&mut self) {
        // The collection holds its own references to the files, so the
        // temporary paths are free to go once the text system is gone.
        for path in self.temp.drain(..) {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl std::fmt::Debug for DirectWriteSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectWriteSystem")
            .field("system_fonts", &self.system_fonts)
            .field("collection", &self.collection.is_some())
            .field("fallback", &self.fallback.is_some())
            .field("registered", &self.registered.len())
            .finish()
    }
}

/// Unused-import guard: the factory type appears only in signatures above.
const _: Option<fn(&IDWriteFactory) -> bool> = None;
