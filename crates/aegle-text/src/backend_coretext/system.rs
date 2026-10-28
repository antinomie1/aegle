//! CoreText state shared by the paragraphs of one text system.
//!
//! A [`CoreTextSystem`] owns the registered application fonts and the
//! generic-family substitution table. `TextSystem::new()` registers font bytes
//! through `CTFontManagerRegisterGraphicsFont` and never asks CoreText for the
//! machine's fonts by name; `TextSystem::system()` does both. Every paragraph
//! asks this type for the `CTFont` of its shaping key, so the font objects are
//! created in one place and reference counted by their owners.
//!
//! # Verification
//!
//! Compile-checked only; see the module documentation of [`super`].

use std::collections::BTreeMap;

use objc2_core_text::CTFont;

use super::fonts::{self, Registered};
use crate::style::Shaping;
use crate::{FontFamily, GenericFamily, TextError};

/// CoreText state shared by every paragraph of one text system.
pub(crate) struct CoreTextSystem {
    /// Whether installed fonts are visible to family lookup.
    system_fonts: bool,
    /// Registered application fonts, kept alive for the process.
    registered: Vec<Registered>,
    /// Family ids handed to the caller, with the family names they mean.
    ids: Vec<(u32, Vec<String>)>,
    /// Generic families as application fonts stand in for them.
    generics: BTreeMap<GenericFamily, Vec<String>>,
    next_id: u32,
}

impl CoreTextSystem {
    /// Creates the system, using the machine's fonts when `system_fonts` is set.
    pub fn new(system_fonts: bool) -> Option<Self> {
        // CoreText itself has no global initialization; the type exists as long
        // as the process does. Nothing can fail at this point, so this always
        // succeeds and a text system always has a backend.
        Some(Self {
            system_fonts,
            registered: Vec::new(),
            ids: Vec::new(),
            generics: BTreeMap::new(),
            next_id: 1,
        })
    }

    /// Resolves a CSS family list to the family a `CTFont` is created with,
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

    /// The font for one shaping key, at its size, weight and slant.
    pub fn font(&self, shaping: &Shaping) -> Option<objc2_core_foundation::CFRetained<CTFont>> {
        let family = self.family(&shaping.families);
        fonts::font(
            &family,
            f64::from(shaping.size),
            shaping.weight,
            shaping.slant,
        )
    }

    /// Registers every face of one application font file.
    pub fn register_fonts(&mut self, data: &[u8]) -> Result<Vec<FontFamily>, TextError> {
        let registered = fonts::register(data)?;
        let id = self.next_id;
        self.next_id += 1;
        self.ids.push((id, registered.families.clone()));
        self.registered.push(registered);
        Ok(vec![FontFamily(id)])
    }

    /// Makes registered `families` stand in for `generic`.
    pub fn set_generic_families(&mut self, generic: GenericFamily, families: &[FontFamily]) {
        let mut names = Vec::new();
        for family in families {
            if let Some((_, registered)) = self.ids.iter().find(|(id, _)| *id == family.0) {
                names.extend(registered.iter().cloned());
            }
        }
        match names.is_empty() {
            true => self.generics.remove(&generic),
            false => self.generics.insert(generic, names),
        };
    }

    /// Releases cached font objects; registered fonts and ids stay valid.
    ///
    /// CoreText's own font cache is not addressable, so this releases only what
    /// this module holds: nothing, beyond what the dropped owners release.
    pub fn trim(&mut self) {
        // Registrations must stay: live paragraphs hold their own font objects,
        // but a later restyle resolves the family again.
    }
}

impl std::fmt::Debug for CoreTextSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreTextSystem")
            .field("system_fonts", &self.system_fonts)
            .field("registered", &self.registered.len())
            .finish()
    }
}
