//! Translation of SDL keycodes and modifier bits into the platform contract.

use aegle_platform::{Key, Modifiers};

// The SDL keycodes this backend names; printable keys are their Unicode code
// point, which is why `Character` needs no table.
const K_RETURN: u32 = 0x0d;
const K_ESCAPE: u32 = 0x1b;
const K_BACKSPACE: u32 = 0x08;
const K_TAB: u32 = 0x09;
const K_DELETE: u32 = 0x7f;
const K_CAPSLOCK: u32 = 0x4000_0039;
/// F1–F12 and F13–F24 are two runs of keycodes; the codes between them name
/// editing and navigation keys, so a single range would misread them.
const K_F1: u32 = 0x4000_003a;
const K_F12: u32 = 0x4000_0045;
const K_F13: u32 = 0x4000_0068;
const K_F24: u32 = 0x4000_0073;
const K_INSERT: u32 = 0x4000_0049;
const K_HOME: u32 = 0x4000_004a;
const K_PAGEUP: u32 = 0x4000_004b;
const K_END: u32 = 0x4000_004d;
const K_PAGEDOWN: u32 = 0x4000_004e;
const K_RIGHT: u32 = 0x4000_004f;
const K_LEFT: u32 = 0x4000_0050;
const K_DOWN: u32 = 0x4000_0051;
const K_UP: u32 = 0x4000_0052;
const K_KP_ENTER: u32 = 0x4000_0058;
const K_APPLICATION: u32 = 0x4000_0065;

// `SDL_Keymod` bits.
const MODE_SHIFT: u16 = 0x0003;
const MODE_CTRL: u16 = 0x00c0;
const MODE_ALT: u16 = 0x0300;
const MODE_GUI: u16 = 0x0c00;
/// `SDL_KMOD_MODE`: the AltGr/level-5 shift, which never means Control.
const MODE_LEVEL5: u16 = 0x4000;

/// The logical key of an SDL3 keycode. Letters arrive lowercase whatever Shift
/// does; the produced text comes from `SDL_EVENT_TEXT_INPUT` instead.
pub(crate) fn key(code: u32) -> Key {
    if (K_F1..=K_F12).contains(&code) {
        return Key::Function((code - K_F1 + 1) as u8);
    }
    if (K_F13..=K_F24).contains(&code) {
        return Key::Function((code - K_F13 + 13) as u8);
    }
    match code {
        K_RETURN | K_KP_ENTER => Key::Enter,
        K_TAB => Key::Tab,
        K_ESCAPE => Key::Escape,
        K_BACKSPACE => Key::Backspace,
        K_DELETE => Key::Delete,
        K_LEFT => Key::Left,
        K_RIGHT => Key::Right,
        K_UP => Key::Up,
        K_DOWN => Key::Down,
        K_HOME => Key::Home,
        K_END => Key::End,
        K_PAGEUP => Key::PageUp,
        K_PAGEDOWN => Key::PageDown,
        K_APPLICATION => Key::ContextMenu,
        K_CAPSLOCK | K_INSERT => Key::Unidentified,
        // Everything below the special-key range is a Unicode code point;
        // control characters are not printable keys.
        code if code < 0x4000_0000 => char::from_u32(code)
            .filter(|c| !c.is_control())
            .map_or(Key::Unidentified, Key::Character),
        _ => Key::Unidentified,
    }
}

/// The contract's modifiers of an SDL key-modifier mask. Lock states are
/// irrelevant to controls, and the level-5 shift (AltGr) suppresses Control so
/// it cannot trigger Ctrl+A and friends.
pub(crate) fn modifiers(value: u16) -> Modifiers {
    Modifiers {
        shift: value & MODE_SHIFT != 0,
        control: value & MODE_CTRL != 0 && value & MODE_LEVEL5 == 0,
        alt: value & MODE_ALT != 0,
        meta: value & MODE_GUI != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_become_logical_keys() {
        assert_eq!(key(K_RETURN), Key::Enter);
        assert_eq!(key(K_KP_ENTER), Key::Enter);
        assert_eq!(key(K_F1), Key::Function(1));
        assert_eq!(key(K_F12), Key::Function(12));
        assert_eq!(key(K_F13), Key::Function(13));
        assert_eq!(key(K_F24), Key::Function(24));
        // Navigation keycodes sit between the two F-key runs.
        assert_eq!(key(K_LEFT), Key::Left);
        assert_eq!(key(K_END), Key::End);
        assert_eq!(key(K_APPLICATION), Key::ContextMenu);
        assert_eq!(key(b'a'.into()), Key::Character('a'));
        assert_eq!(key('中'.into()), Key::Character('中'));
        assert_eq!(key(K_ESCAPE), Key::Escape);
    }

    #[test]
    fn altgr_reports_no_control() {
        assert!(modifiers(MODE_CTRL).control);
        assert!(!modifiers(MODE_CTRL | MODE_LEVEL5).control);
        assert!(modifiers(MODE_ALT | MODE_LEVEL5).alt);
        assert!(!modifiers(MODE_SHIFT).alt);
    }
}
