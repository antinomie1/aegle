//! Platform events into a window's `Ui`, the same on every platform.
//!
//! Each window has one focus domain, owned by the seat that last focused it:
//! other seats' keys and pointers are ignored until that seat leaves.

use crate::{Native, native::Entry};
use aegle_platform::{Event, ImeEvent, Platform, PointerKind as Pointer, SeatId};
use aegle_text::Selection;
use aegle_ui::{
    ClipboardRequest, ImeEdit, ImeRequest, Key, KeyInput, Modifiers, PointerId, PointerKind,
    Result, Size,
};
use std::time::Instant;

/// Pointer motion, which a batch may apply before a single refresh.
pub(crate) fn is_motion(event: &Event) -> bool {
    matches!(
        event,
        Event::Pointer {
            kind: Pointer::Move,
            ..
        }
    )
}

impl Entry {
    /// The seat requests act for: the focus owner, or seat 0, which
    /// single-seat platforms use and others ignore.
    fn acting_seat(&self) -> SeatId {
        self.seat.unwrap_or(SeatId(0))
    }

    /// Whether `seat` may drive this window, adopting it when none does.
    fn adopt(&mut self, seat: SeatId) -> bool {
        *self.seat.get_or_insert(seat) == seat
    }

    /// Starts momentum from the last 100 ms of finger scrolling; a pause before
    /// the lift, or a single sample, leaves the view where it stopped.
    #[cfg(feature = "motion")]
    fn release_flick(&mut self, time: u32, position: aegle_ui::Point) {
        let samples = std::mem::take(&mut self.flick);
        let recent: Vec<_> = samples
            .iter()
            .filter(|(at, _)| time.wrapping_sub(*at) <= 100)
            .collect();
        let (Some(first), Some(last)) = (recent.first(), recent.last()) else {
            return;
        };
        let span = last.0.wrapping_sub(first.0) as f32 / 1000.0;
        if recent.len() < 2 || span <= 0.0 {
            return;
        }
        let (x, y) = recent[1..]
            .iter()
            .fold((0.0, 0.0), |(x, y), (_, d)| (x + d.x, y + d.y));
        self.ui
            .fling(position, aegle_ui::Point::new(x / span, y / span));
    }

    pub fn event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Configure { info, .. } => {
                // A scale change can keep the logical size; candidate windows
                // still need the new physical placement.
                self.ui.request_ime_sync();
                #[cfg(access)]
                self.accessibility.configure(&info);
                self.ui
                    .resize(Size::new(info.size.width as f32, info.size.height as f32));
            }
            Event::Redraw { .. } => self.ready = true,
            Event::KeyboardFocus { seat, focused, .. } => {
                if focused {
                    if self.seat != Some(seat) {
                        self.ui.window_focus(false)?;
                        self.seat = Some(seat);
                        self.modifiers = Modifiers::default();
                    }
                    self.ui.window_focus(true)?;
                } else if self.seat == Some(seat) {
                    self.ui.window_focus(false)?;
                    self.seat = None;
                }
            }
            Event::Key {
                seat,
                key,
                text,
                pressed,
                repeat,
                modifiers,
                time,
                ..
            } if self.seat == Some(seat) => {
                self.modifiers = modifiers;
                let time = self.clock.at(time);
                self.ui.key_at(
                    KeyInput {
                        key,
                        text: text.as_deref().unwrap_or(""),
                        modifiers,
                        pressed,
                        repeat,
                    },
                    time,
                )?;
            }
            Event::Text { seat, text, .. } if self.seat == Some(seat) => {
                // Text already reflects native keyboard translation, including
                // AltGr and dead keys; the editor rejects control characters.
                self.ui.key(KeyInput {
                    key: Key::Unidentified,
                    text: &text,
                    modifiers: Modifiers::default(),
                    pressed: true,
                    repeat: false,
                })?;
            }
            Event::Modifiers {
                seat, modifiers, ..
            } if self.adopt(seat) => self.modifiers = modifiers,
            Event::Pointer {
                seat,
                position,
                kind,
                time,
                ..
            } if self.adopt(seat) => {
                let at = time.map_or_else(Instant::now, |time| self.clock.at(time));
                let kind = match kind {
                    Pointer::Move => PointerKind::Move,
                    Pointer::Down { clicks } => PointerKind::Down { clicks },
                    Pointer::Up => PointerKind::Up,
                    Pointer::ButtonDown(button) => PointerKind::ButtonDown(button),
                    Pointer::ButtonUp(button) => PointerKind::ButtonUp(button),
                    Pointer::Leave => return self.ui.pointer_leave(),
                    Pointer::Scroll { delta } => {
                        #[cfg(feature = "motion")]
                        if let Some(time) = time {
                            if self.flick.len() == 8 {
                                self.flick.remove(0);
                            }
                            self.flick.push((time, delta));
                        }
                        return self.ui.wheel(position, delta, self.modifiers, at);
                    }
                    Pointer::ScrollEnd => {
                        #[cfg(feature = "motion")]
                        if let Some(time) = time {
                            self.release_flick(time, position);
                        }
                        return Ok(());
                    }
                };
                let id = PointerId(u64::from(seat.0));
                self.ui.pointer_at(id, kind, position, self.modifiers, at)?;
            }
            Event::Touch {
                seat,
                finger,
                position,
                time,
                phase,
                ..
            } => {
                // Fingers are pointers of their own: seat and finger identify them.
                let pointer =
                    PointerId((1 << 40) | (u64::from(seat.0) << 16) | u64::from(finger as u16));
                self.ui.touch(pointer, phase, position, time)?;
            }
            Event::Ime { seat, event, .. } => {
                // Text-input focus is authoritative even when the seat has not
                // produced a keyboard event (for example an on-screen IME).
                if self.seat.is_none() && matches!(event, ImeEvent::Entered) {
                    self.seat = Some(seat);
                    self.ui.window_focus(true)?;
                }
                if self.seat != Some(seat) {
                    return Ok(());
                }
                match event {
                    ImeEvent::Update(update) => self.ui.ime(ImeEdit {
                        delete_before: update.delete_before,
                        delete_after: update.delete_after,
                        commit: update.commit.as_deref(),
                        preedit: &update.preedit.text,
                        cursor: update
                            .preedit
                            .cursor
                            .map(|(anchor, focus)| Selection { anchor, focus }),
                    })?,
                    ImeEvent::Left => self.ui.ime_left()?,
                    ImeEvent::Entered => self.ui.request_ime_sync(),
                }
            }
            Event::Clipboard { seat, text, .. } if self.seat == Some(seat) => {
                self.ui.paste(&text)?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Routes a drag over the window to its drop targets and answers the
/// platform; other events are returned.
pub(crate) fn drag(backend: &mut Native, entry: &Entry, event: Event) -> Result<Option<Event>> {
    match event {
        Event::Drag {
            window,
            seat,
            position,
        } => {
            let accept = entry.ui.drag_motion(position);
            backend.accept_drag(window, seat, accept)?;
        }
        Event::DragLeave { .. } => entry.ui.drag_leave(),
        Event::Drop { position, data, .. } => {
            entry.ui.drop_data(position, data);
        }
        event => return Ok(Some(event)),
    }
    Ok(None)
}

/// Starts a drag a control asked for from the window's seat. Some platforms
/// return only when the drag ends.
pub(crate) fn start_drag(
    backend: &mut Native,
    entry: &Entry,
    data: aegle_ui::DragData,
) -> Result<()> {
    Ok(backend.start_drag(entry.id, entry.acting_seat(), &data)?)
}

/// Offers or requests the clipboard of the window's seat; read text
/// arrives as a clipboard event.
pub(crate) fn clipboard(
    backend: &mut Native,
    entry: &Entry,
    request: ClipboardRequest,
) -> Result<()> {
    let seat = entry.acting_seat();
    match request {
        ClipboardRequest::Write(text) => backend.set_clipboard(entry.id, seat, &text)?,
        ClipboardRequest::Read => backend.request_clipboard(entry.id, seat)?,
    }
    Ok(())
}

/// The engine's editor state as a platform request.
pub(crate) fn ime_request(request: Option<ImeRequest>) -> Option<aegle_platform::ImeRequest> {
    request.map(|request| aegle_platform::ImeRequest {
        surrounding: request.surrounding,
        cursor: request.selection.focus,
        anchor: request.selection.anchor,
        cursor_rect: request.cursor_rect,
        multiline: request.multiline,
        input_method: request.input_method,
    })
}
