//! Pango font map, its Fontconfig configuration and generic-family mapping.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_void};

use super::sys::{FC_SET_APPLICATION, PangoLib, pango};

pub(crate) struct PangoSystem {
    pub(super) fontmap: *mut c_void,
    pub(super) context: *mut c_void,
    fc_config: *mut c_void,
    /// Whether `fc_config` is ours to destroy rather than Pango's default.
    owns_config: bool,
    /// Family names each registered font provided, by registered id.
    registered: Vec<(u32, Vec<String>)>,
    /// Generic families as application fonts stand in for them.
    generics: Vec<(crate::GenericFamily, Vec<String>)>,
    temp_files: Vec<String>,
}

impl PangoSystem {
    /// Creates the font map; without system fonts it gets an empty configuration
    /// that only registered application fonts populate.
    pub fn new(system_fonts: bool) -> Option<Self> {
        let p = pango()?;
        unsafe {
            // Fontconfig initializes lazily and is not reentrant; do it once
            // before any configuration is created or queried.
            static FONT_CONFIG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if !*FONT_CONFIG.get_or_init(|| (p.fc_init)() != 0) {
                return None;
            }
            let fontmap = (p.ft2_font_map_new)();
            (p.ft2_font_map_set_resolution)(fontmap, 72.0, 72.0);
            let (fc_config, owns_config) = match system_fonts {
                true => ((p.pango_fc_font_map_get_config)(fontmap), false),
                false => {
                    let config = (p.fc_config_create)();
                    (p.pango_fc_font_map_set_config)(fontmap, config);
                    (config, true)
                }
            };
            let context = (p.font_map_create_context)(fontmap);
            Some(Self {
                fontmap,
                context,
                fc_config,
                owns_config,
                registered: Vec::new(),
                generics: Vec::new(),
                temp_files: Vec::new(),
            })
        }
    }

    /// Makes registered `families` stand in for `generic`, as the fallback
    /// font list of every paragraph naming that generic family.
    pub fn set_generic_families(
        &mut self,
        generic: crate::GenericFamily,
        families: &[crate::FontFamily],
    ) {
        let mut names = Vec::new();
        for family in families {
            if let Some((_, list)) = self.registered.iter().find(|(id, _)| *id == family.0) {
                names.extend(list.iter().cloned());
            }
        }
        self.generics.retain(|(existing, _)| *existing != generic);
        if !names.is_empty() {
            self.generics.push((generic, names));
        }
    }

    /// Expands a CSS family list, replacing generic names a registration stands in for.
    pub(super) fn resolve_families(&self, css: &str, locale: Option<&str>) -> Option<CString> {
        let mut resolved: Vec<String> = Vec::new();
        for token in css.split(',') {
            let name = token.trim().trim_matches(['"', '\'']);
            if name.is_empty() {
                continue;
            }
            let generic = match name.to_ascii_lowercase().as_str() {
                "serif" => Some(crate::GenericFamily::Serif),
                "sans-serif" => Some(crate::GenericFamily::SansSerif),
                "monospace" => Some(crate::GenericFamily::Monospace),
                "cursive" => Some(crate::GenericFamily::Cursive),
                "fantasy" => Some(crate::GenericFamily::Fantasy),
                "system-ui" => Some(crate::GenericFamily::SystemUi),
                "emoji" => Some(crate::GenericFamily::Emoji),
                _ => None,
            };
            match generic.and_then(|g| {
                self.generics
                    .iter()
                    .find(|(existing, _)| *existing == g)
                    .map(|(_, names)| names)
            }) {
                Some(names) => resolved.extend(names.iter().map(|n| format!("\"{n}\""))),
                None => resolved.push(format!("\"{name}\"")),
            }
        }
        if resolved.is_empty() {
            return None;
        }
        let _ = locale;
        CString::new(resolved.join(", ")).ok()
    }

    /// The family names a just-added font file provided.
    fn families_of(&self, p: &PangoLib, path: &str) -> Vec<String> {
        let mut names = Vec::new();
        unsafe {
            let set = (p.fc_config_get_fonts)(self.fc_config, FC_SET_APPLICATION);
            if set.is_null() {
                return names;
            }
            let set = &*set;
            let fc_file = CString::new("file").unwrap();
            let fc_family = CString::new("family").unwrap();
            for index in 0..set.nfont.max(0) as usize {
                let pattern = *set.fonts.add(index);
                let mut file: *mut c_char = std::ptr::null_mut();
                if (p.fc_pattern_get_string)(pattern, fc_file.as_ptr(), 0, &mut file) != 0 {
                    continue;
                }
                if file.is_null() || CStr::from_ptr(file).to_string_lossy() != path {
                    continue;
                }
                let mut index = 0;
                loop {
                    let mut family: *mut c_char = std::ptr::null_mut();
                    if (p.fc_pattern_get_string)(pattern, fc_family.as_ptr(), index, &mut family)
                        != 0
                    {
                        break;
                    }
                    if family.is_null() {
                        break;
                    }
                    names.push(CStr::from_ptr(family).to_string_lossy().into_owned());
                    index += 1;
                }
            }
        }
        names
    }

    pub fn register_fonts(
        &mut self,
        data: &[u8],
    ) -> Result<Vec<crate::FontFamily>, crate::TextError> {
        if data.len() < 4 {
            return Err(crate::TextError::InvalidFont);
        }
        let tag = u32::from_be_bytes(data[0..4].try_into().unwrap());
        if tag != 0x00010000 && tag != 0x4f54544f && tag != 0x74746366 && tag != 0x74727565 {
            return Err(crate::TextError::InvalidFont);
        }
        let p = pango().ok_or(crate::TextError::InvalidFont)?;
        static FONT_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = FONT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = format!("/tmp/aegle-font-{}-{}.otf", std::process::id(), id);
        fs::write(&path, data).map_err(|_| crate::TextError::InvalidFont)?;
        self.temp_files.push(path.clone());
        let c_path = CString::new(path.as_str()).map_err(|_| crate::TextError::InvalidFont)?;
        unsafe {
            if (p.fc_config_app_font_add_file)(self.fc_config, c_path.as_ptr()) == 0 {
                let _ = fs::remove_file(&path);
                self.temp_files.retain(|kept| kept != &path);
                return Err(crate::TextError::InvalidFont);
            }
            (p.pango_fc_font_map_config_changed)(self.fontmap);
        }
        let names = self.families_of(p, &path);
        if names.is_empty() {
            // A font Fontconfig cannot name would silently fall back to another
            // family, so registering it is an explicit failure.
            return Err(crate::TextError::InvalidFont);
        }
        self.registered.push((id as u32, names));
        Ok(vec![crate::FontFamily(id as u32)])
    }

    pub fn trim(&mut self) {
        #[cfg(feature = "scene")]
        super::paint::release_fonts();
    }
}

impl Drop for PangoSystem {
    fn drop(&mut self) {
        if let Some(p) = pango() {
            unsafe {
                (p.g_object_unref)(self.context);
                (p.g_object_unref)(self.fontmap);
                if self.owns_config && !self.fc_config.is_null() {
                    (p.fc_config_destroy)(self.fc_config);
                }
            }
        }
    }
}
