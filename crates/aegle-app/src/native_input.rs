use crate::native::Entry;
use aegle_platform_wayland::{Event, ImeEvent, Keysym, PointerEventKind, TouchPhase, WindowId};
use aegle_text::Selection;
use aegle_ui::{
    ClipboardRequest, ImeEdit, ImeRequest, Key, KeyInput, Modifiers, Point, PointerId, PointerKind,
    Result, Size,
};
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
        | Event::Touch { window, .. }
        | Event::Ime { window, .. }
        | Event::Clipboard { window, .. } => Some(*window),
        Event::Wake | Event::Preferences(_) | Event::Error(_) => None,
    }
}

impl Entry {
    /// Starts momentum from the last 100 ms of finger scrolling; a pause before
    /// the lift, or a single sample, leaves the view where it stopped.
    #[cfg(feature = "motion")]
    fn release_flick(&mut self, time: u32, position: Point) -> Result<()> {
        let samples = std::mem::take(&mut self.flick);
        let recent: Vec<_> = samples
            .iter()
            .filter(|(at, _)| time.wrapping_sub(*at) <= 100)
            .collect();
        let (Some(first), Some(last)) = (recent.first(), recent.last()) else {
            return Ok(());
        };
        let span = last.0.wrapping_sub(first.0) as f32 / 1000.0;
        if recent.len() < 2 || span <= 0.0 {
            return Ok(());
        }
        let (x, y) = recent[1..]
            .iter()
            .fold((0.0, 0.0), |(x, y), (_, d)| (x + d.x, y + d.y));
        self.ui.fling(position, Point::new(x / span, y / span))
    }

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
                    PointerEventKind::Axis {
                        #[cfg(feature = "motion")]
                        time,
                        horizontal,
                        vertical,
                        ..
                    } => {
                        let delta =
                            Point::new(horizontal.absolute as f32, vertical.absolute as f32);
                        #[cfg(feature = "motion")]
                        if horizontal.stop || vertical.stop {
                            return self.release_flick(time, position);
                        } else {
                            if self.flick.len() == 8 {
                                self.flick.remove(0);
                            }
                            self.flick.push((time, delta));
                        }
                        return self.ui.scroll_by(position, delta);
                    }
                    _ => return Ok(()),
                };
                self.ui.pointer(id, kind, position, self.modifiers)?;
            }
            Event::Touch {
                seat,
                id,
                position,
                time,
                phase,
                ..
            } => {
                // Fingers are pointers of their own: seat and finger identify them.
                let pointer = PointerId(
                    (1 << 40) | (u64::from(seat.id().protocol_id()) << 16) | (id as u16 as u64),
                );
                let phase = match phase {
                    TouchPhase::Down => aegle_ui::TouchPhase::Down,
                    TouchPhase::Move => aegle_ui::TouchPhase::Move,
                    TouchPhase::Up => aegle_ui::TouchPhase::Up,
                    TouchPhase::Cancel => aegle_ui::TouchPhase::Cancel,
                };
                self.ui.touch(pointer, phase, position, time)?;
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
            Event::Clipboard { seat, text, .. } if self.seat.as_ref() == Some(&seat) => {
                self.ui.paste(&text)?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Offers or requests the clipboard of the window's active keyboard seat.
pub(crate) fn clipboard(
    backend: &mut crate::platform::Wayland,
    entry: &Entry,
    request: ClipboardRequest,
) -> Result<()> {
    let Some(seat) = &entry.seat else {
        return Ok(());
    };
    match request {
        ClipboardRequest::Write(text) => backend.set_clipboard(seat, &text)?,
        ClipboardRequest::Read => backend.request_clipboard(entry.id, seat)?,
    }
    Ok(())
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

pub(crate) fn ime_request(
    request: Option<ImeRequest>,
) -> Option<aegle_platform_wayland::ImeRequest> {
    use aegle_platform_wayland::{ImeCause, ImeHints, ImeRequest};
    request.map(|request| ImeRequest {
        surrounding: request.surrounding,
        cursor: request.selection.focus,
        anchor: request.selection.anchor,
        cursor_rect: request.cursor_rect,
        hints: if request.multiline {
            ImeHints::Multiline
        } else {
            ImeHints::empty()
        },
        cause: if request.input_method {
            ImeCause::InputMethod
        } else {
            ImeCause::Other
        },
        ..Default::default()
    })
}
