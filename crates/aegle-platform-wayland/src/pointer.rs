//! Pointer frames: focus bookkeeping, cursor shape on enter, and window events.

use aegle_types::{Cursor, Point};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use wayland_client::{Connection, QueueHandle, protocol::wl_pointer::WlPointer};

use super::icon;
use crate::{Error, Event, State};

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(index) = self.input.seats.iter().position(|input| {
            input
                .pointer
                .as_ref()
                .is_some_and(|themed| themed.pointer() == pointer)
        }) else {
            return;
        };
        for event in events {
            let Some(window) = self.window_id(&event.surface) else {
                continue;
            };
            let input = &mut self.input.seats[index];
            if let PointerEventKind::Press { serial, .. } = event.kind {
                self.clipboard.input(&input.seat, serial);
            }
            let position = Point {
                x: event.position.0 as f32,
                y: event.position.1 as f32,
            };
            input.pointer_focus = match event.kind {
                PointerEventKind::Leave { .. } => None,
                _ => Some((window, position)),
            };
            if matches!(event.kind, PointerEventKind::Enter { .. }) {
                // Entering needs a fresh cursor for the serial; keep the window's.
                let shape = self
                    .windows
                    .iter()
                    .find(|w| w.id == window)
                    .map_or(Cursor::Default, |w| w.cursor);
                if let Err(error) = input
                    .pointer
                    .as_ref()
                    .expect("registered pointer")
                    .set_cursor(conn, icon(shape))
                {
                    self.events.push_back(Event::Error(Error::backend(error)));
                }
            }
            self.events.push_back(Event::Pointer {
                window,
                seat: input.seat.clone(),
                position,
                kind: event.kind.clone(),
            });
        }
    }
}
