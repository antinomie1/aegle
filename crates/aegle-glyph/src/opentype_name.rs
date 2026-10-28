//! Minimal OpenType face description: family, subfamily, weight and slant.
//!
//! A platform font request (the CoreText backend registering application
//! fonts) needs the family name and style bits, which only the font file knows. This parser reads the `name`
//! and `OS/2` tables and nothing else. Every offset and length is checked
//! against the file, counts are capped, and a file it cannot understand yields
//! `None` so the caller reports an unsupported face instead of guessing.

/// Largest name record count this parser accepts.
const MAX_NAMES: usize = 512;
/// Largest single name string in UTF-16 code units.
const MAX_NAME_UNITS: usize = 1024;

/// What a font file says about its own face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceDescription {
    /// Typographic family name (`name` ID 1), else the legacy family (ID 16).
    pub family: String,
    /// Style name (`name` ID 2), such as `Regular` or `Bold Italic`.
    pub subfamily: String,
    /// `OS/2.usWeightClass` when present, else a weight inferred from the style.
    pub weight: u16,
    /// Whether the face is italic or oblique.
    pub italic: bool,
}

/// Reads the face description of the font at `index` in `bytes`.
///
/// `bytes` may be a single font or a collection; `index` selects the face.
/// Returns `None` for malformed data, a missing `name` table or a missing
/// family name. Never allocates more than the bounded strings it returns.
pub fn describe(bytes: &[u8], index: u32) -> Option<FaceDescription> {
    let face = face_offset(bytes, index)?;
    let mut tables = Tables::default();
    read_directory(bytes, face, &mut tables)?;

    let name = tables.name.and_then(|(offset, len)| {
        let table = bytes.get(offset..offset.checked_add(len)?)?;
        read_names(table)
    })?;
    let family = name.family?;
    let subfamily = name.subfamily.unwrap_or_default();
    let (weight, mut italic) = match tables.os2 {
        Some((offset, len)) => {
            let table = bytes.get(offset..offset.checked_add(len)?)?;
            read_os2(table).unwrap_or((400, false))
        }
        None => (400, false),
    };
    italic |= subfamily.to_ascii_lowercase().contains("italic")
        || subfamily.to_ascii_lowercase().contains("oblique");
    let weight = match weight {
        0 => infer_weight(&subfamily),
        weight => weight,
    };
    Some(FaceDescription {
        family,
        subfamily,
        weight,
        italic,
    })
}

fn infer_weight(subfamily: &str) -> u16 {
    let style = subfamily.to_ascii_lowercase();
    if style.contains("thin") {
        100
    } else if style.contains("extralight") || style.contains("ultralight") {
        200
    } else if style.contains("light") {
        300
    } else if style.contains("medium") {
        500
    } else if style.contains("semibold") || style.contains("demibold") {
        600
    } else if style.contains("extrabold") || style.contains("ultrabold") {
        800
    } else if style.contains("black") || style.contains("heavy") {
        900
    } else if style.contains("bold") {
        700
    } else {
        400
    }
}

#[derive(Default)]
struct Tables {
    name: Option<(usize, usize)>,
    os2: Option<(usize, usize)>,
}

/// Offset of the requested face in a single font or a collection.
fn face_offset(bytes: &[u8], index: u32) -> Option<usize> {
    let tag = bytes.get(0..4)?;
    if tag == b"ttcf" {
        let count = u32::from_be_bytes(bytes.get(8..12)?.try_into().ok()?) as usize;
        if index as usize >= count {
            return None;
        }
        let start = 12usize.checked_add(index as usize * 4)?;
        let offset = u32::from_be_bytes(bytes.get(start..start + 4)?.try_into().ok()?);
        usize::try_from(offset).ok()
    } else {
        match index {
            0 => Some(0),
            _ => None,
        }
    }
}

fn read_directory(bytes: &[u8], face: usize, tables: &mut Tables) -> Option<()> {
    let count = u16::from_be_bytes(bytes.get(face + 4..face + 6)?.try_into().ok()?) as usize;
    if count > 512 {
        return None;
    }
    for i in 0..count {
        let record = face.checked_add(12 + i * 16)?;
        let tag = bytes.get(record..record + 4)?;
        let offset = u32::from_be_bytes(bytes.get(record + 8..record + 12)?.try_into().ok()?);
        let len = u32::from_be_bytes(bytes.get(record + 12..record + 16)?.try_into().ok()?);
        let entry = (usize::try_from(offset).ok()?, usize::try_from(len).ok()?);
        match tag {
            b"name" => tables.name = Some(entry),
            b"OS/2" => tables.os2 = Some(entry),
            _ => {}
        }
    }
    Some(())
}

struct Names {
    family: Option<String>,
    subfamily: Option<String>,
}

/// Decodes the best family/subfamily pair from a `name` table.
///
/// Preference is Windows English, then Windows any language, then Unicode, then
/// Macintosh; a later, better match replaces an earlier, worse one.
fn read_names(table: &[u8]) -> Option<Names> {
    let count = u16::from_be_bytes(table.get(2..4)?.try_into().ok()?) as usize;
    let storage = u16::from_be_bytes(table.get(4..6)?.try_into().ok()?) as usize;
    if count > MAX_NAMES {
        return None;
    }
    let mut family: Option<(u8, String)> = None;
    let mut subfamily: Option<(u8, String)> = None;
    for i in 0..count {
        let record = 6 + i * 12;
        let platform = u16::from_be_bytes(table.get(record..record + 2)?.try_into().ok()?);
        let encoding = u16::from_be_bytes(table.get(record + 2..record + 4)?.try_into().ok()?);
        let language = u16::from_be_bytes(table.get(record + 4..record + 6)?.try_into().ok()?);
        let name = u16::from_be_bytes(table.get(record + 6..record + 8)?.try_into().ok()?);
        let len = u16::from_be_bytes(table.get(record + 8..record + 10)?.try_into().ok()?) as usize;
        let offset =
            u16::from_be_bytes(table.get(record + 10..record + 12)?.try_into().ok()?) as usize;
        if len > MAX_NAME_UNITS * 4 {
            return None;
        }
        let start = storage.checked_add(offset)?;
        let raw = table.get(start..start.checked_add(len)?)?;
        let rank = match (platform, language) {
            (3, 0x409) => 0,
            (3, _) => 1,
            (0, _) => 2,
            (1, 0) => 3,
            _ => continue,
        };
        let text = match platform {
            3 | 0 => decode_utf16(raw)?,
            _ => raw.iter().map(|&b| b as char).collect(),
        };
        if text.is_empty() {
            continue;
        }
        let slot = match name {
            1 => &mut family,
            2 => &mut subfamily,
            _ => continue,
        };
        if slot.as_ref().is_none_or(|(best, _)| rank < *best) {
            *slot = Some((rank, text));
        }
        // `encoding` only distinguishes Unicode sub-encodings we decode alike.
        let _ = encoding;
    }
    Some(Names {
        family: family.map(|(_, text)| text),
        subfamily: subfamily.map(|(_, text)| text),
    })
}

fn decode_utf16(raw: &[u8]) -> Option<String> {
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    String::from_utf16(&units).ok()
}

/// `usWeightClass` and the italic/oblique bits of `fsSelection`.
fn read_os2(table: &[u8]) -> Option<(u16, bool)> {
    let weight = u16::from_be_bytes(table.get(4..6)?.try_into().ok()?);
    let selection = u16::from_be_bytes(table.get(62..64)?.try_into().ok()?);
    Some((weight, selection & 0b1_0000_0001 != 0))
}
