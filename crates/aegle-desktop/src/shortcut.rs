//! Shortcut triggers: modifiers and one key, parsed from text such as
//! `"Ctrl+Alt+K"`.

use std::{fmt, str::FromStr};

/// Named keys besides letters, digits and F1–F24: (name accepted, XKB keysym
/// name, Windows virtual-key code).
const NAMED: [(&str, &str, u16); 15] = [
    ("Space", "space", 0x20),
    ("Enter", "Return", 0x0D),
    ("Tab", "Tab", 0x09),
    ("Escape", "Escape", 0x1B),
    ("Backspace", "BackSpace", 0x08),
    ("Delete", "Delete", 0x2E),
    ("Insert", "Insert", 0x2D),
    ("Home", "Home", 0x24),
    ("End", "End", 0x23),
    ("PageUp", "Prior", 0x21),
    ("PageDown", "Next", 0x22),
    ("Left", "Left", 0x25),
    ("Right", "Right", 0x27),
    ("Up", "Up", 0x26),
    ("Down", "Down", 0x28),
];

/// The key of a trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    /// An uppercase ASCII letter or a digit.
    Char(u8),
    /// F1–F24.
    Function(u8),
    /// An index into `NAMED`.
    Named(usize),
}

/// Modifiers and one key: a letter, a digit, F1–F24 or one of Space, Enter,
/// Tab, Escape, Backspace, Delete, Insert, Home, End, PageUp, PageDown, Left,
/// Right, Up and Down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// Control.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// The logo (Super, Windows) key.
    pub logo: bool,
    key: Key,
}

/// The text was not `Modifier+…+Key` with known names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTrigger;

impl fmt::Display for InvalidTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a shortcut trigger such as Ctrl+Alt+K")
    }
}

impl std::error::Error for InvalidTrigger {}

impl FromStr for Trigger {
    type Err = InvalidTrigger;

    /// Parses `+`-separated modifiers (Ctrl, Alt, Shift, Super or Logo, in
    /// any case) followed by the key.
    fn from_str(text: &str) -> Result<Self, InvalidTrigger> {
        let mut parts: Vec<_> = text.split('+').map(str::trim).collect();
        let key = parts.pop().ok_or(InvalidTrigger)?;
        let mut trigger = Self {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: parse_key(key).ok_or(InvalidTrigger)?,
        };
        for part in parts {
            let flag = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut trigger.ctrl,
                "alt" => &mut trigger.alt,
                "shift" => &mut trigger.shift,
                "super" | "logo" | "win" => &mut trigger.logo,
                _ => return Err(InvalidTrigger),
            };
            *flag = true;
        }
        Ok(trigger)
    }
}

fn parse_key(key: &str) -> Option<Key> {
    if let [byte] = key.as_bytes()
        && byte.is_ascii_alphanumeric()
    {
        return Some(Key::Char(byte.to_ascii_uppercase()));
    }
    if let Some(number) = key.strip_prefix(['F', 'f'])
        && let Ok(n @ 1..=24) = number.parse()
    {
        return Some(Key::Function(n));
    }
    NAMED
        .iter()
        .position(|(name, ..)| name.eq_ignore_ascii_case(key))
        .map(Key::Named)
}

impl Trigger {
    /// The form of the XDG shortcuts specification, such as `CTRL+ALT+k`.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn xdg(self) -> String {
        let mut text = String::new();
        for (on, name) in [
            (self.ctrl, "CTRL+"),
            (self.alt, "ALT+"),
            (self.shift, "SHIFT+"),
            (self.logo, "LOGO+"),
        ] {
            if on {
                text.push_str(name);
            }
        }
        match self.key {
            Key::Char(byte) => text.push(byte.to_ascii_lowercase() as char),
            Key::Function(n) => text.push_str(&format!("F{n}")),
            Key::Named(index) => text.push_str(NAMED[index].1),
        }
        text
    }

    /// The macOS virtual key code (`kVK_…`), by key position on an ANSI
    /// keyboard; `None` for F21–F24, which Mac keyboards lack.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn mac_key_code(self) -> Option<u16> {
        // Letters A–Z and digits 0–9 in key-position order.
        const LETTERS: [u16; 26] = [
            0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D,
            0x1F, 0x23, 0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
        ];
        const DIGITS: [u16; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
        const FUNCTIONS: [u16; 20] = [
            0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F, 0x69, 0x6B,
            0x71, 0x6A, 0x40, 0x4F, 0x50, 0x5A,
        ];
        // In `NAMED` order; Insert is the Help key's position.
        const NAMED_CODES: [u16; 15] = [
            0x31, 0x24, 0x30, 0x35, 0x33, 0x75, 0x72, 0x73, 0x77, 0x74, 0x79, 0x7B, 0x7C, 0x7E,
            0x7D,
        ];
        match self.key {
            Key::Char(byte @ b'A'..=b'Z') => Some(LETTERS[usize::from(byte - b'A')]),
            Key::Char(byte) => Some(DIGITS[usize::from(byte - b'0')]),
            Key::Function(n) => FUNCTIONS.get(usize::from(n) - 1).copied(),
            Key::Named(index) => Some(NAMED_CODES[index]),
        }
    }

    /// The Windows virtual-key code.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn virtual_key(self) -> u16 {
        match self.key {
            Key::Char(byte) => byte.into(),
            Key::Function(n) => 0x6F + u16::from(n),
            Key::Named(index) => NAMED[index].2,
        }
    }
}
