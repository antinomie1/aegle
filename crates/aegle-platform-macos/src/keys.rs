//! Translation of AppKit key events: logical keys, modifiers and typed text.
use aegle_platform::{Key, Modifiers};
use objc2_app_kit::{NSEvent, NSEventModifierFlags};

/// The modifiers of an event. Caps Lock and the function key are not
/// modifiers for shortcuts.
pub(crate) fn modifiers(event: &NSEvent) -> Modifiers {
    let flags = event.modifierFlags();
    Modifiers {
        shift: flags.contains(NSEventModifierFlags::Shift),
        control: flags.contains(NSEventModifierFlags::Control),
        alt: flags.contains(NSEventModifierFlags::Option),
        meta: flags.contains(NSEventModifierFlags::Command),
    }
}

/// Characters in the private-use block AppKit uses for arrows and function keys.
fn is_function_char(ch: char) -> bool {
    ('\u{f700}'..='\u{f8ff}').contains(&ch)
}

/// The logical key of an event: special keys by their layout-independent
/// virtual key code (`kVK_*`), others by the character the key types without
/// modifiers, letters uppercased like a shortcut name.
pub(crate) fn key(event: &NSEvent) -> Key {
    match event.keyCode() {
        0x24 | 0x4c => return Key::Enter,
        0x30 => return Key::Tab,
        0x35 => return Key::Escape,
        0x33 => return Key::Backspace,
        0x75 => return Key::Delete,
        0x7b => return Key::Left,
        0x7c => return Key::Right,
        0x7d => return Key::Down,
        0x7e => return Key::Up,
        0x73 => return Key::Home,
        0x77 => return Key::End,
        0x74 => return Key::PageUp,
        0x79 => return Key::PageDown,
        0x6e => return Key::ContextMenu,
        // F1 to F20 are not consecutive.
        0x7a => return Key::Function(1),
        0x78 => return Key::Function(2),
        0x63 => return Key::Function(3),
        0x76 => return Key::Function(4),
        0x60 => return Key::Function(5),
        0x61 => return Key::Function(6),
        0x62 => return Key::Function(7),
        0x64 => return Key::Function(8),
        0x65 => return Key::Function(9),
        0x6d => return Key::Function(10),
        0x67 => return Key::Function(11),
        0x6f => return Key::Function(12),
        0x69 => return Key::Function(13),
        0x6b => return Key::Function(14),
        0x71 => return Key::Function(15),
        0x6a => return Key::Function(16),
        0x40 => return Key::Function(17),
        0x4f => return Key::Function(18),
        0x50 => return Key::Function(19),
        0x5a => return Key::Function(20),
        _ => {}
    }
    let typed = event.charactersIgnoringModifiers();
    let first = typed
        .as_ref()
        .and_then(|text| text.to_string().chars().next());
    match first {
        Some(ch) if !ch.is_control() && !is_function_char(ch) => {
            Key::Character(ch.to_ascii_uppercase())
        }
        _ => Key::Unidentified,
    }
}

/// The text a key press types when no input client translates it: the
/// event's characters unless they are control or function-key codes.
pub(crate) fn typed_text(event: &NSEvent) -> Option<String> {
    let text = event.characters()?.to_string();
    (!text.is_empty()
        && !text
            .chars()
            .any(|ch| ch.is_control() || is_function_char(ch)))
    .then_some(text)
}
