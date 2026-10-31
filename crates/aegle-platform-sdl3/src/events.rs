//! Translation of `SDL_EVENT_*` into the platform contract.
#![allow(unsafe_code)]

use aegle_platform::{Event, ImeEvent, ImeUpdate, PointerButton, PointerKind, Preedit, TouchPhase};
use aegle_types::Point;

use aegle_platform::WindowId;

use crate::{
    keys, sys,
    window::{SEAT, Sdl3, drag_data},
};

/// Logical pixels one wheel notch scrolls, about three lines of body text.
const WHEEL_PIXELS: f32 = 40.0;

impl Sdl3 {
    /// Translates one SDL event into queued contract events.
    pub(crate) fn translate(&mut self, event: &sys::Event) {
        // SAFETY: `type` is the union's discriminant and every field read below
        // belongs to the event type SDL wrote.
        let kind = unsafe { event.kind };
        if kind == self.wake_type {
            self.sync_portal();
            self.emit(Event::Wake);
            return;
        }
        match kind {
            sys::EVENT_QUIT => {
                // SDL reports the application's quit (a desktop request or a
                // handled signal) rather than one window's close.
                let windows: Vec<_> = self.windows.iter().map(|window| window.id).collect();
                for window in windows {
                    self.emit(Event::Close { window });
                }
            }
            sys::EVENT_SYSTEM_THEME_CHANGED => self.theme_changed(),
            sys::EVENT_WINDOW_SHOWN => self.window_event(event, |platform, id, _| {
                if let Ok(window) = platform.window_mut(id) {
                    window.shown = true;
                }
                platform.reconfigure(id);
            }),
            sys::EVENT_WINDOW_HIDDEN => self.window_event(event, |platform, id, _| {
                if let Ok(window) = platform.window_mut(id) {
                    window.shown = false;
                }
                platform.reconfigure(id);
            }),
            sys::EVENT_WINDOW_EXPOSED => self.window_event(event, |platform, id, _| {
                // The compositor discarded this window's pixels: redraw all of
                // it rather than only the changed region.
                if let Ok(window) = platform.window_mut(id) {
                    window.drawn = None;
                    window.dirty = true;
                }
                platform.queue_redraws(std::time::Instant::now());
            }),
            sys::EVENT_WINDOW_RESIZED
            | sys::EVENT_WINDOW_PIXEL_SIZE_CHANGED
            | sys::EVENT_WINDOW_MINIMIZED
            | sys::EVENT_WINDOW_MAXIMIZED
            | sys::EVENT_WINDOW_RESTORED => {
                self.window_event(event, |platform, id, _| platform.reconfigure(id))
            }
            sys::EVENT_WINDOW_DISPLAY_CHANGED | sys::EVENT_WINDOW_DISPLAY_SCALE_CHANGED => self
                .window_event(event, |platform, id, _| {
                    platform.update_interval(id);
                    platform.reconfigure(id);
                }),
            sys::EVENT_WINDOW_ENTER_FULLSCREEN | sys::EVENT_WINDOW_LEAVE_FULLSCREEN => {
                self.window_event(event, |platform, id, _| platform.reconfigure(id))
            }
            sys::EVENT_WINDOW_FOCUS_GAINED => self.window_event(event, |platform, id, _| {
                // SDL delivers text events only while a window has text input
                // started, so it follows the keyboard focus.
                platform.start_input(id);
                platform.reconfigure(id);
                platform.emit(Event::KeyboardFocus {
                    window: id,
                    seat: SEAT,
                    focused: true,
                });
            }),
            sys::EVENT_WINDOW_FOCUS_LOST => self.window_event(event, |platform, id, _| {
                platform.stop_input(id);
                platform.reconfigure(id);
                platform.emit(Event::KeyboardFocus {
                    window: id,
                    seat: SEAT,
                    focused: false,
                });
            }),
            sys::EVENT_WINDOW_MOUSE_ENTER => self.window_event(event, |platform, id, _| {
                let cursor = platform.window_mut(id).map(|window| {
                    window.mouse_focus = true;
                    window.cursor
                });
                if let Ok(cursor) = cursor {
                    platform.apply_cursor(cursor);
                }
            }),
            sys::EVENT_WINDOW_MOUSE_LEAVE => self.window_event(event, |platform, id, time| {
                if let Ok(window) = platform.window_mut(id) {
                    window.mouse_focus = false;
                }
                platform.emit(Event::Pointer {
                    window: id,
                    seat: SEAT,
                    position: Point::new(0.0, 0.0),
                    kind: PointerKind::Leave,
                    time: Some(time),
                });
            }),
            sys::EVENT_WINDOW_CLOSE_REQUESTED => self.window_event(event, |platform, id, _| {
                platform.emit(Event::Close { window: id });
            }),
            // Occlusion and destruction need no host action: an occluded window
            // keeps its last frame, and a window SDL destroyed is one this
            // backend already removed.
            sys::EVENT_WINDOW_OCCLUDED | sys::EVENT_WINDOW_DESTROYED if false => {}
            sys::EVENT_WINDOW_OCCLUDED => {}
            sys::EVENT_KEY_DOWN | sys::EVENT_KEY_UP => {
                // SAFETY: the event type selects the keyboard structure.
                let key = unsafe { &event.key };
                let Some(id) = self.id_of(key.window_id) else {
                    return;
                };
                self.emit(Event::Key {
                    window: id,
                    seat: SEAT,
                    key: keys::key(key.key),
                    // SDL reports produced text as a separate event.
                    text: None,
                    pressed: key.down != 0,
                    repeat: key.repeat != 0,
                    modifiers: keys::modifiers(key.modifiers),
                    time: millis(key.timestamp),
                });
            }
            sys::EVENT_TEXT_EDITING => {
                // SAFETY: the event type selects the editing structure.
                let edit = unsafe { &event.edit };
                let Some(id) = self.id_of(edit.window_id) else {
                    return;
                };
                if !self.editing(id) {
                    return;
                }
                // SAFETY: SDL lends the string for the duration of this event.
                let text = unsafe { sys::string(edit.text) }.unwrap_or_default();
                let cursor = preedit_cursor(edit.start, edit.length, &text);
                self.emit(Event::Ime {
                    window: id,
                    seat: SEAT,
                    event: ImeEvent::Update(ImeUpdate {
                        delete_before: 0,
                        delete_after: 0,
                        commit: None,
                        preedit: Preedit { text, cursor },
                    }),
                });
            }
            sys::EVENT_TEXT_INPUT => {
                // SAFETY: the event type selects the text structure.
                let input = unsafe { &event.text };
                let Some(id) = self.id_of(input.window_id) else {
                    return;
                };
                // SAFETY: SDL lends the string for the duration of this event.
                let Some(text) = (unsafe { sys::string(input.text) }).filter(|t| !t.is_empty())
                else {
                    return;
                };
                if self.editing(id) {
                    // An editor is focused: this is a composition commit, which
                    // replaces the preedit instead of inserting beside it.
                    self.emit(Event::Ime {
                        window: id,
                        seat: SEAT,
                        event: ImeEvent::Update(ImeUpdate {
                            delete_before: 0,
                            delete_after: 0,
                            commit: Some(text),
                            preedit: Preedit::default(),
                        }),
                    });
                } else {
                    self.emit(Event::Text {
                        window: id,
                        seat: SEAT,
                        text,
                    });
                }
            }
            sys::EVENT_MOUSE_MOTION => {
                // SAFETY: the event type selects the motion structure.
                let motion = unsafe { &event.motion };
                if motion.which == sys::TOUCH_MOUSEID {
                    // Finger contacts arrive as `wl_touch`-like touch events;
                    // the synthesized pointer would handle them twice.
                    return;
                }
                let Some(id) = self.id_of(motion.window_id) else {
                    return;
                };
                self.emit(Event::Pointer {
                    window: id,
                    seat: SEAT,
                    position: Point::new(motion.x, motion.y),
                    kind: PointerKind::Move,
                    time: Some(millis(motion.timestamp)),
                });
            }
            sys::EVENT_MOUSE_BUTTON_DOWN | sys::EVENT_MOUSE_BUTTON_UP => {
                // SAFETY: the event type selects the button structure.
                let button = unsafe { &event.button };
                if button.which == sys::TOUCH_MOUSEID {
                    return;
                }
                let Some(id) = self.id_of(button.window_id) else {
                    return;
                };
                let Some(kind) = button_kind(button.button, button.down != 0, button.clicks) else {
                    return;
                };
                self.emit(Event::Pointer {
                    window: id,
                    seat: SEAT,
                    position: Point::new(button.x, button.y),
                    kind,
                    time: Some(millis(button.timestamp)),
                });
            }
            sys::EVENT_MOUSE_WHEEL => {
                // SAFETY: the event type selects the wheel structure.
                let wheel = unsafe { &event.wheel };
                if wheel.which == sys::TOUCH_MOUSEID {
                    return;
                }
                let Some(id) = self.id_of(wheel.window_id) else {
                    return;
                };
                // SDL's vertical amount is positive away from the user; the
                // contract counts positive as down. Its `direction` already
                // reflects the desktop's natural-scrolling choice.
                let delta = Point::new(wheel.x * WHEEL_PIXELS, -wheel.y * WHEEL_PIXELS);
                if delta.x == 0.0 && delta.y == 0.0 {
                    return;
                }
                self.emit(Event::Pointer {
                    window: id,
                    seat: SEAT,
                    position: Point::new(wheel.mouse_x, wheel.mouse_y),
                    kind: PointerKind::Scroll { delta },
                    time: Some(millis(wheel.timestamp)),
                });
            }
            sys::EVENT_FINGER_DOWN
            | sys::EVENT_FINGER_MOTION
            | sys::EVENT_FINGER_UP
            | sys::EVENT_FINGER_CANCELED => {
                // SAFETY: the event type selects the finger structure.
                let finger = unsafe { &event.finger };
                let Some(id) = self.id_of(finger.window_id) else {
                    return;
                };
                let Some(window) = self.windows.iter().find(|window| window.id == id) else {
                    return;
                };
                let size = window.info.size;
                let phase = match kind {
                    sys::EVENT_FINGER_DOWN => TouchPhase::Down,
                    sys::EVENT_FINGER_MOTION => TouchPhase::Move,
                    sys::EVENT_FINGER_UP => TouchPhase::Up,
                    _ => TouchPhase::Cancel,
                };
                self.emit(Event::Touch {
                    window: id,
                    seat: SEAT,
                    // A finger identity only has to be unique among the seat's
                    // active contacts, which SDL's low half already is.
                    finger: finger.finger_id as u32,
                    position: Point::new(
                        finger.x * size.width as f32,
                        finger.y * size.height as f32,
                    ),
                    time: millis(finger.timestamp),
                    phase,
                });
            }
            sys::EVENT_DROP_BEGIN => {
                // SAFETY: the event type selects the drop structure.
                let drop = unsafe { &event.drop };
                if let Some(window) = self
                    .id_of(drop.window_id)
                    .and_then(|id| self.window_mut(id).ok())
                {
                    window.drag_accept = None;
                    window.drag_files.clear();
                    window.drag_text = None;
                    window.drag_at = None;
                }
            }
            sys::EVENT_DROP_POSITION => {
                // SAFETY: the event type selects the drop structure.
                let drop = unsafe { &event.drop };
                let Some(id) = self.id_of(drop.window_id) else {
                    return;
                };
                let position = Point::new(drop.x, drop.y);
                if let Ok(window) = self.window_mut(id) {
                    window.drag_at = Some(position);
                }
                self.emit(Event::Drag {
                    window: id,
                    seat: SEAT,
                    position,
                });
            }
            sys::EVENT_DROP_FILE | sys::EVENT_DROP_TEXT => {
                // SAFETY: the event type selects the drop structure.
                let drop = unsafe { &event.drop };
                let Some(id) = self.id_of(drop.window_id) else {
                    return;
                };
                // SAFETY: SDL lends the string for the duration of this event.
                let data = unsafe { sys::string(drop.data) };
                let position = Point::new(drop.x, drop.y);
                if let Ok(window) = self.window_mut(id) {
                    window.drag_at = Some(position);
                    match (kind, data) {
                        (sys::EVENT_DROP_FILE, Some(path)) => {
                            window.drag_files.push(std::path::PathBuf::from(path))
                        }
                        (sys::EVENT_DROP_TEXT, Some(text)) => window.drag_text = Some(text),
                        _ => {}
                    }
                }
            }
            sys::EVENT_DROP_COMPLETE => {
                // SAFETY: the event type selects the drop structure.
                let drop = unsafe { &event.drop };
                let Some(id) = self.id_of(drop.window_id) else {
                    return;
                };
                let Some(window) = self.windows.iter_mut().find(|window| window.id == id) else {
                    return;
                };
                // SDL cannot refuse a drop, so a target that said no is told the
                // drag left instead of receiving its data.
                let refused = window.drag_accept == Some(false);
                window.drag_accept = None;
                let position = window.drag_at.take().unwrap_or_default();
                let data = drag_data(&mut window.drag_files, &mut window.drag_text);
                match data.filter(|_| !refused) {
                    Some(data) => self.emit(Event::Drop {
                        window: id,
                        seat: SEAT,
                        position,
                        data,
                    }),
                    None => self.emit(Event::DragLeave {
                        window: id,
                        seat: SEAT,
                    }),
                }
            }
            sys::EVENT_KEYMAP_CHANGED => {}
            _ => {}
        }
    }

    /// Runs `apply` for the window an SDL window event names, with the event's
    /// platform time.
    fn window_event(&mut self, event: &sys::Event, apply: impl FnOnce(&mut Self, WindowId, u32)) {
        // SAFETY: only window events reach this function.
        let window = unsafe { &event.window };
        let Some(id) = self.id_of(window.window_id) else {
            return;
        };
        apply(self, id, millis(window.timestamp));
    }

    /// Whether an editable control is focused in this window, so text belongs
    /// to an input-method session rather than to plain keyboard input.
    fn editing(&self, id: WindowId) -> bool {
        self.window(id)
            .map(|window| window.ime.is_some())
            .unwrap_or(false)
    }
}

/// A button transition; the primary button carries SDL's own click count, and
/// buttons no control uses have none.
fn button_kind(button: u8, down: bool, clicks: u8) -> Option<PointerKind> {
    if button == 1 {
        return Some(if down {
            PointerKind::Down {
                clicks: clicks.max(1),
            }
        } else {
            PointerKind::Up
        });
    }
    let button = match button {
        2 => PointerButton::Middle,
        3 => PointerButton::Secondary,
        4 => PointerButton::Back,
        5 => PointerButton::Forward,
        _ => return None,
    };
    Some(if down {
        PointerKind::ButtonDown(button)
    } else {
        PointerKind::ButtonUp(button)
    })
}

/// The selection an SDL editing event describes, as byte offsets into `text`.
/// An unset or out-of-bounds range hides the cursor instead of truncating.
fn preedit_cursor(start: i32, length: i32, text: &str) -> Option<(usize, usize)> {
    if start < 0 {
        return None;
    }
    let start = start as usize;
    let end = start.checked_add(length.max(0) as usize)?;
    (end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end))
        .then_some((start, end))
}

/// SDL's nanosecond timestamp as the contract's platform milliseconds.
fn millis(nanoseconds: u64) -> u32 {
    (nanoseconds / 1_000_000) as u32
}
