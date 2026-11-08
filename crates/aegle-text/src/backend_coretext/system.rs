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
//! See the verification notes in the module documentation of [`super`].

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
    ///
    /// CoreText knows no generic names and silently substitutes its default
    /// face for a family it lacks, so the first family of the list that is
    /// installed wins, and a generic family without a registration stands for
    /// the macOS fonts that CSS engines use for it. With none present, the
    /// first name is kept and CoreText's substitution applies. A system built
    /// without the machine's fonts matches registrations only, and has no
    /// family (`None`) until one exists.
    pub fn family(&self, css: &str) -> Option<String> {
        let mut first = None;
        for token in css.split(',') {
            let name = token.trim().trim_matches(['"', '\'']);
            if name.is_empty() {
                continue;
            }
            let (generic, system): (_, &[&str]) = match name.to_ascii_lowercase().as_str() {
                "serif" => (Some(GenericFamily::Serif), &["Times"]),
                "sans-serif" => (
                    Some(GenericFamily::SansSerif),
                    &["Helvetica Neue", "Helvetica"],
                ),
                "monospace" => (Some(GenericFamily::Monospace), &["Menlo", "Courier"]),
                "cursive" => (Some(GenericFamily::Cursive), &["Apple Chancery"]),
                "fantasy" => (Some(GenericFamily::Fantasy), &["Papyrus"]),
                "system-ui" => (Some(GenericFamily::SystemUi), &[".AppleSystemUIFont"]),
                "emoji" => (Some(GenericFamily::Emoji), &["Apple Color Emoji"]),
                _ => (None, &[]),
            };
            if let Some(registered) = generic
                .and_then(|generic| self.generics.get(&generic))
                .and_then(|names| names.first())
            {
                return Some(registered.clone());
            }
            if !self.system_fonts {
                if generic.is_none()
                    && let Some(found) = self.registered_family(name)
                {
                    return Some(found.to_owned());
                }
                continue;
            }
            let candidates = match generic {
                Some(_) => system,
                None => std::slice::from_ref(&name),
            };
            if let Some(found) = candidates.iter().find(|family| fonts::installed(family)) {
                return Some((*found).to_owned());
            }
            first.get_or_insert(name);
        }
        if !self.system_fonts {
            // Without the machine's fonts an unmatched list falls back to the
            // first registration, as CoreText would substitute an installed
            // face; with none registered nothing can shape.
            return self
                .registered
                .first()
                .and_then(|registered| registered.families.first())
                .cloned();
        }
        Some(first.unwrap_or("sans-serif").to_owned())
    }

    /// The registered family named `name`, compared case-insensitively.
    fn registered_family(&self, name: &str) -> Option<&str> {
        self.registered
            .iter()
            .flat_map(|registered| &registered.families)
            .find(|family| family.eq_ignore_ascii_case(name))
            .map(String::as_str)
    }

    /// The font for one shaping key, at its size, weight and slant.
    pub fn font(&self, shaping: &Shaping) -> Option<objc2_core_foundation::CFRetained<CTFont>> {
        let family = self.family(&shaping.families)?;
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
