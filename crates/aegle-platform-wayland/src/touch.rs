//! `wl_touch`: fingers become [`Event::Touch`] with their window and seat.

use aegle_types::Point;
use smithay_client_toolkit::seat::touch::TouchHandler;
use wayland_client::{
    Connection, QueueHandle,
    protocol::{wl_surface::WlSurface, wl_touch::WlTouch},
};

use crate::{Event, State, TouchPhase};

impl State {
    /// Ends every active finger of a seat, as when the device disappears.
    pub(super) fn cancel_fingers(&mut self, index: usize) {
        let input = &mut self.input.seats[index];
        for (id, window) in input.fingers.drain(..) {
            self.events.push_back(Event::Touch {
                window,
                seat: input.seat.clone(),
                id,
                position: Point::default(),
                time: 0,
                phase: TouchPhase::Cancel,
            });
        }
    }

    fn touch_seat(&self, touch: &WlTouch) -> Option<usize> {
        self.input
            .seats
            .iter()
            .position(|input| input.touch.as_ref() == Some(touch))
    }

    fn touch_event(
        &mut self,
        index: usize,
        id: i32,
        position: (f64, f64),
        time: u32,
        phase: TouchPhase,
    ) {
        let input = &mut self.input.seats[index];
        let Some(&(_, window)) = input.fingers.iter().find(|(finger, _)| *finger == id) else {
            return;
        };
        if matches!(phase, TouchPhase::Up | TouchPhase::Cancel) {
            input.fingers.retain(|(finger, _)| *finger != id);
        }
        self.events.push_back(Event::Touch {
            window,
            seat: input.seat.clone(),
            id,
            position: Point {
                x: position.0 as f32,
                y: position.1 as f32,
            },
            time,
            phase,
        });
    }
}

impl TouchHandler for State {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        serial: u32,
        time: u32,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        let (Some(index), Some(window)) = (self.touch_seat(touch), self.window_id(&surface)) else {
            return;
        };
        let input = &mut self.input.seats[index];
        self.clipboard.input(&input.seat, serial);
        input.fingers.retain(|(finger, _)| *finger != id);
        input.fingers.push((id, window));
        self.touch_event(index, id, position, time, TouchPhase::Down);
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        _: u32,
        time: u32,
        id: i32,
    ) {
        if let Some(index) = self.touch_seat(touch) {
            self.touch_event(index, id, (0.0, 0.0), time, TouchPhase::Up);
        }
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        if let Some(index) = self.touch_seat(touch) {
            self.touch_event(index, id, position, time, TouchPhase::Move);
        }
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
    }

    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlTouch, _: i32, _: f64) {}

    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, touch: &WlTouch) {
        if let Some(index) = self.touch_seat(touch) {
            self.cancel_fingers(index);
        }
    }
}
