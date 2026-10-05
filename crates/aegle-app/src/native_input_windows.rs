use crate::native::Entry;
use crate::platform::{Event, ImeEvent, WindowId};
use crate::{ImeEdit, Key, KeyInput, Modifiers, PointerId, PointerKind, Result, Size};
use aegle_text::Selection;

pub(crate) fn target(event: &Event) -> Option<WindowId> {
    match event {
        Event::Configure { window, .. }
        | Event::Redraw { window }
        | Event::Close { window }
        | Event::KeyboardFocus { window, .. }
        | Event::Key { window, .. }
        | Event::Text { window, .. }
        | Event::Pointer { window, .. }
        | Event::Ime { window, .. } => Some(*window),
        Event::Wake | Event::Error(_) => None,
    }
}

impl Entry {
    pub fn event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Configure { info, .. } => {
                // DPI can change while the rounded logical extent stays equal.
                // IMM candidate placement still needs the new physical scale.
                self.ui.request_ime_sync()?;
                #[cfg(feature = "windows-accessibility")]
                if self.access_scale != f64::from(info.scale) {
                    self.access_scale = f64::from(info.scale);
                    self.initial_access = true;
                }
                self.ui
                    .resize(Size::new(info.size.width as f32, info.size.height as f32))?;
            }
            Event::Redraw { .. } => self.ready = true,
            Event::KeyboardFocus { focused, .. } => {
                self.modifiers = Modifiers::default();
                self.ui.window_focus(focused)?;
            }
            Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } => {
                self.modifiers = normalize(modifiers);
                let mut command_modifiers = self.modifiers;
                // AltGr arrives as Ctrl+Alt; it must not invoke Ctrl+A/Z/Y.
                command_modifiers.control &= !command_modifiers.alt;
                self.ui.key(KeyInput {
                    key: key_id(key),
                    text: "",
                    modifiers: command_modifiers,
                    pressed,
                    repeat,
                })?;
            }
            Event::Text { text, .. } => {
                // WM_CHAR already reflects native keyboard translation, including
                // AltGr and dead keys. Shortcut control characters are rejected by
                // the shared editor; do not suppress valid AltGr text a second time.
                self.ui.key(KeyInput {
                    key: Key::Unidentified,
                    text: &text,
                    modifiers: Modifiers::default(),
                    pressed: true,
                    repeat: false,
                })?;
            }
            Event::Pointer {
                position,
                kind,
                modifiers,
                ..
            } => {
                self.modifiers = normalize(modifiers);
                let kind = match kind {
                    crate::platform::PointerKind::Move => PointerKind::Move,
                    crate::platform::PointerKind::Down { clicks } => PointerKind::Down { clicks },
                    crate::platform::PointerKind::Up => PointerKind::Up,
                    crate::platform::PointerKind::Leave => return self.ui.pointer_leave(),
                    crate::platform::PointerKind::Scroll { delta } => {
                        return self.ui.scroll_by(position, delta);
                    }
                };
                self.ui
                    .pointer(PointerId(0), kind, position, self.modifiers)?;
            }
            Event::Ime { event, .. } => match event {
                ImeEvent::Update(update) => self.ui.ime(ImeEdit {
                    delete_before: 0,
                    delete_after: 0,
                    commit: update.commit.as_deref(),
                    preedit: &update.preedit.text,
                    cursor: update
                        .preedit
                        .cursor
                        .map(|(anchor, focus)| Selection { anchor, focus }),
                })?,
                ImeEvent::Left => self.ui.ime_left()?,
                ImeEvent::Entered => self.ui.request_ime_sync()?,
            },
            _ => {}
        }
        Ok(())
    }
}

fn normalize(modifiers: crate::platform::Modifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        alt: modifiers.alt,
        meta: modifiers.meta,
    }
}

fn key_id(key: u32) -> Key {
    match key {
        0x0d => Key::Enter,
        0x09 => Key::Tab,
        0x1b => Key::Escape,
        0x08 => Key::Backspace,
        0x2e => Key::Delete,
        0x25 => Key::Left,
        0x27 => Key::Right,
        0x26 => Key::Up,
        0x28 => Key::Down,
        0x24 => Key::Home,
        0x23 => Key::End,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x20 => Key::Character(' '),
        0x30..=0x39 | 0x41..=0x5a => Key::Character(char::from_u32(key).unwrap()),
        _ => Key::Unidentified,
    }
}

/// Applies a clipboard request synchronously; a paste lands before refresh.
pub(crate) fn clipboard(
    backend: &mut crate::platform::Win32,
    entry: &Entry,
    request: crate::ClipboardRequest,
) -> Result<()> {
    match request {
        crate::ClipboardRequest::Write(text) => backend.set_clipboard(entry.id, &text)?,
        crate::ClipboardRequest::Read => {
            if let Some(text) = backend.clipboard_text(entry.id)? {
                entry.ui.paste(&text)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn ime_request(
    request: Option<crate::ImeRequest>,
) -> Option<crate::platform::ImeRequest> {
    request.map(|request| crate::platform::ImeRequest {
        cursor_rect: request.cursor_rect,
    })
}
