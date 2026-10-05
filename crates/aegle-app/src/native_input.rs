use crate::native::Entry;
use crate::{ImeEdit, Key, KeyInput, Modifiers, PointerId, PointerKind, Result, Size};
use aegle_platform_wayland::{Event, ImeEvent, Keysym, PointerEventKind, WindowId};
use aegle_text::Selection;
use wayland_client::Proxy;

pub(crate) fn target(event: &Event) -> Option<WindowId> {
    match event {
        Event::Configure { window, .. }
        | Event::Redraw { window }
        | Event::Close { window }
        | Event::KeyboardFocus { window, .. }
        | Event::Key { window, .. }
        | Event::Modifiers { window, .. }
        | Event::Pointer { window, .. }
        | Event::Ime { window, .. } => Some(*window),
        Event::Wake | Event::Error(_) => None,
    }
}

impl Entry {
    pub fn event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Configure { info, .. } => {
                self.ui
                    .resize(Size::new(info.size.width as f32, info.size.height as f32))?;
                #[cfg(feature = "unix-accessibility")]
                self.accessibility.set_window_focused(info.active);
            }
            Event::Redraw { .. } => self.ready = true,
            Event::KeyboardFocus { seat, focused, .. } => {
                if focused {
                    if self.seat.as_ref() != Some(&seat) {
                        self.ui.window_focus(false)?;
                        self.seat = Some(seat);
                        self.modifiers = Modifiers::default();
                    }
                    self.ui.window_focus(true)?;
                } else if self.seat.as_ref() == Some(&seat) {
                    self.ui.window_focus(false)?;
                    self.seat = None;
                }
            }
            Event::Key {
                seat,
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } if self.seat.as_ref() == Some(&seat) => {
                self.modifiers = normalize(modifiers);
                self.ui.key(KeyInput {
                    key: key_id(key.keysym),
                    text: key.utf8.as_deref().unwrap_or(""),
                    modifiers: self.modifiers,
                    pressed,
                    repeat,
                })?;
            }
            Event::Modifiers {
                seat, modifiers, ..
            } if self.seat.as_ref() == Some(&seat) => self.modifiers = normalize(modifiers),
            Event::Pointer {
                seat,
                position,
                kind,
                ..
            } => {
                if self.seat.is_none() {
                    self.seat = Some(seat.clone());
                }
                if self.seat.as_ref() != Some(&seat) {
                    return Ok(());
                }
                let id = PointerId(u64::from(seat.id().protocol_id()));
                let kind = match kind {
                    PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                        PointerKind::Move
                    }
                    PointerEventKind::Press { button: 0x110, .. } => {
                        PointerKind::Down { clicks: 1 }
                    }
                    PointerEventKind::Release { button: 0x110, .. } => PointerKind::Up,
                    PointerEventKind::Leave { .. } => return self.ui.pointer_leave(),
                    PointerEventKind::Axis { vertical, .. } => {
                        return self.ui.scroll(position, vertical.absolute as f32);
                    }
                    _ => return Ok(()),
                };
                self.ui.pointer(id, kind, position, self.modifiers)?;
            }
            Event::Ime { seat, event, .. } => {
                // Text-input focus is authoritative even when the seat has not
                // produced a wl_keyboard event (for example an on-screen IME).
                if self.seat.is_none() && matches!(event, ImeEvent::Entered) {
                    self.seat = Some(seat.clone());
                    self.ui.window_focus(true)?;
                }
                if self.seat.as_ref() != Some(&seat) {
                    return Ok(());
                }
                match event {
                    ImeEvent::Update(update) => self.ui.ime(ImeEdit {
                        delete_before: update.delete_before as usize,
                        delete_after: update.delete_after as usize,
                        commit: update.commit.as_deref(),
                        preedit: &update.preedit.text,
                        cursor: update
                            .preedit
                            .cursor
                            .map(|(anchor, focus)| Selection { anchor, focus }),
                    })?,
                    ImeEvent::Left => self.ui.ime_left()?,
                    ImeEvent::Entered => self.ui.request_ime_sync()?,
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn normalize(value: aegle_platform_wayland::Modifiers) -> Modifiers {
    Modifiers {
        shift: value.shift,
        control: value.ctrl,
        alt: value.alt,
        meta: value.logo,
    }
}

fn key_id(value: Keysym) -> Key {
    match value {
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::Escape => Key::Escape,
        Keysym::BackSpace => Key::Backspace,
        Keysym::Delete => Key::Delete,
        Keysym::Left => Key::Left,
        Keysym::Right => Key::Right,
        Keysym::Up => Key::Up,
        Keysym::Down => Key::Down,
        Keysym::Home => Key::Home,
        Keysym::End => Key::End,
        Keysym::Page_Up => Key::PageUp,
        Keysym::Page_Down => Key::PageDown,
        value => value
            .key_char()
            .map(Key::Character)
            .unwrap_or(Key::Unidentified),
    }
}
