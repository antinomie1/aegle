//! Seat-local input state. SCTK translates physical keys; repeat timers are explicitly owned.

#[path = "pointer.rs"]
mod pointer;
#[path = "repeat.rs"]
mod repeat;
#[path = "touch.rs"]
mod touch;
use repeat::Repeat;

use aegle_types::{Cursor, Point};
use smithay_client_toolkit::{
    delegate_dispatch2,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{
            KeyEvent, KeyboardHandler, Keymap, Keysym, Modifiers, RawModifiers, RepeatInfo,
        },
        pointer::{CursorIcon, PointerEventKind, ThemeSpec, ThemedPointer},
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    protocol::{wl_keyboard, wl_seat::WlSeat, wl_surface::WlSurface, wl_touch::WlTouch},
};

use crate::{Error, Event, ImeEvent, State, WindowId};

fn icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Text => CursorIcon::Text,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::ResizeHorizontal => CursorIcon::EwResize,
        Cursor::ResizeVertical => CursorIcon::NsResize,
    }
}

struct SeatInput {
    seat: WlSeat,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    touch: Option<WlTouch>,
    /// Active fingers and the windows they touched down in.
    fingers: Vec<(i32, WindowId)>,
    focus: Option<(WindowId, WlSurface)>,
    pointer_focus: Option<(WindowId, Point)>,
    modifiers: Modifiers,
    repeat: Repeat,
}

impl Drop for SeatInput {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take()
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        self.pointer.take();
        if let Some(touch) = self.touch.take()
            && touch.version() >= 3
        {
            touch.release();
        }
        if self.seat.version() >= 5 {
            self.seat.release();
        }
    }
}

#[derive(Default)]
pub(crate) struct InputState {
    seats: Vec<SeatInput>,
}

impl State {
    /// SCTK only calls `new_seat` for globals arriving after initial binding.
    pub(crate) fn init_input(&mut self, conn: &Connection, qh: &QueueHandle<Self>) {
        for seat in self.seat_state.seats() {
            self.new_seat(conn, qh, seat);
        }
    }

    /// Theme cursors need a newly scaled bitmap when crossing output boundaries.
    pub(crate) fn update_input_cursor_scale(&mut self, conn: &Connection, surface: &WlSurface) {
        if let Some(window) = self.window_id(surface) {
            self.apply_cursor(conn, window);
        }
    }

    /// Shows the window's cursor on every pointer currently inside it.
    pub(crate) fn apply_cursor(&mut self, conn: &Connection, window: WindowId) {
        let Some(shape) = self
            .windows
            .iter()
            .find(|w| w.id == window)
            .map(|w| w.cursor)
        else {
            return;
        };
        for input in &self.input.seats {
            if let Some(pointer) = &input.pointer
                && input.pointer_focus.is_some_and(|(id, _)| id == window)
                && let Err(error) = pointer.set_cursor(conn, icon(shape))
            {
                self.events.push_back(Event::Error(Error::backend(error)));
            }
        }
    }

    /// Cancel timers before releasing input objects or the event source.
    pub(crate) fn shutdown_input(&mut self, _: &Connection, _: &QueueHandle<Self>) {
        self.input.seats.clear();
    }

    /// Clear focus and cancel repeat before destroying a window.
    pub(crate) fn remove_input_window(
        &mut self,
        window: WindowId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        for index in 0..self.input.seats.len() {
            if self.input.seats[index]
                .focus
                .as_ref()
                .is_some_and(|(id, _)| *id == window)
            {
                self.cancel_keyboard_focus(index);
            }
            if self.input.seats[index]
                .pointer_focus
                .is_some_and(|(id, _)| id == window)
            {
                self.cancel_pointer_focus(index);
            }
        }
    }

    fn cancel_keyboard_focus(&mut self, index: usize) {
        let input = &mut self.input.seats[index];
        input.repeat.cancel();
        input.modifiers = Modifiers::default();
        if let Some((window, _)) = input.focus.take() {
            self.events.push_back(Event::KeyboardFocus {
                window,
                seat: input.seat.clone(),
                focused: false,
            });
        }
    }

    fn cancel_pointer_focus(&mut self, index: usize) {
        let input = &mut self.input.seats[index];
        if let Some((window, position)) = input.pointer_focus.take() {
            self.events.push_back(Event::Pointer {
                window,
                seat: input.seat.clone(),
                position,
                kind: PointerEventKind::Leave { serial: 0 },
            });
        }
    }

    fn key_event(
        &mut self,
        keyboard: &wl_keyboard::WlKeyboard,
        key: KeyEvent,
        pressed: bool,
        repeat: bool,
    ) {
        if let Some(input) = self
            .input
            .seats
            .iter()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
            && let Some((window, _)) = input.focus
        {
            self.events.push_back(Event::Key {
                window,
                seat: input.seat.clone(),
                key,
                pressed,
                repeat,
                modifiers: input.modifiers,
            });
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: WlSeat) {
        self.ime.add_seat(&seat, qh);
        self.clipboard.add_seat(qh, &seat);
        self.input.seats.push(SeatInput {
            seat,
            keyboard: None,
            pointer: None,
            touch: None,
            fingers: Vec::new(),
            focus: None,
            pointer_focus: None,
            modifiers: Modifiers::default(),
            repeat: Repeat::new(&self.loop_handle),
        });
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let input = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.seat == seat)
            .expect("registered seat");
        match capability {
            Capability::Keyboard => match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => input.keyboard = Some(keyboard),
                Err(error) => self.events.push_back(Event::Error(Error::backend(error))),
            },
            Capability::Pointer => {
                let surface = self.compositor.create_surface(qh);
                match self.seat_state.get_pointer_with_theme::<State, ()>(
                    qh,
                    &seat,
                    self.shm.wl_shm(),
                    surface.clone(),
                    ThemeSpec::System,
                ) {
                    Ok(pointer) => input.pointer = Some(pointer),
                    Err(error) => {
                        surface.destroy();
                        self.events.push_back(Event::Error(Error::backend(error)));
                    }
                }
            }
            Capability::Touch => match self.seat_state.get_touch(qh, &seat) {
                Ok(touch) => input.touch = Some(touch),
                Err(error) => self.events.push_back(Event::Error(Error::backend(error))),
            },
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let Some(index) = self.input.seats.iter().position(|input| input.seat == seat) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                self.cancel_keyboard_focus(index);
                self.input.seats[index].repeat.remove_keyboard();
                if let Some(keyboard) = self.input.seats[index].keyboard.take()
                    && keyboard.version() >= 3
                {
                    keyboard.release();
                }
            }
            Capability::Pointer => {
                self.cancel_pointer_focus(index);
                self.input.seats[index].pointer.take();
            }
            Capability::Touch => {
                self.cancel_fingers(index);
                if let Some(touch) = self.input.seats[index].touch.take()
                    && touch.version() >= 3
                {
                    touch.release();
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.clipboard.remove_seat(&seat);
        if let Some(window) = self.ime.remove_seat(&seat) {
            self.events.push_back(Event::Ime {
                window,
                seat: seat.clone(),
                event: ImeEvent::Left,
            });
        }
        if let Some(index) = self.input.seats.iter().position(|input| input.seat == seat) {
            self.cancel_keyboard_focus(index);
            self.cancel_pointer_focus(index);
            self.cancel_fingers(index);
            self.input.seats.swap_remove(index);
        }
    }
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        let Some(window) = self.window_id(surface) else {
            return;
        };
        if let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
        {
            input.focus = Some((window, surface.clone()));
            self.events.push_back(Event::KeyboardFocus {
                window,
                seat: input.seat.clone(),
                focused: true,
            });
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
        if let Some(index) = self
            .input
            .seats
            .iter()
            .position(|input| input.keyboard.as_ref() == Some(keyboard))
        {
            self.cancel_keyboard_focus(index);
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        key: KeyEvent,
    ) {
        if let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
        {
            self.clipboard.input(&input.seat, serial);
            if input.focus.is_some()
                && let Err(error) = input.repeat.press(&key, keyboard)
            {
                self.events.push_back(Event::Error(error));
            }
        }
        self.key_event(keyboard, key, true, false);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        key: KeyEvent,
    ) {
        let Some(input) = self
            .input
            .seats
            .iter()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
        else {
            return;
        };
        let key = input.repeat.server_event(key);
        self.key_event(keyboard, key, true, true);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        key: KeyEvent,
    ) {
        if let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
        {
            input.repeat.release(key.raw_code);
        }
        self.key_event(keyboard, key, false, false);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        raw: RawModifiers,
        layout: u32,
    ) {
        if let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
        {
            input.modifiers = modifiers;
            input.repeat.modifiers(raw, layout);
            if let Some((window, _)) = input.focus {
                self.events.push_back(Event::Modifiers {
                    window,
                    seat: input.seat.clone(),
                    modifiers,
                });
            }
        }
    }

    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        if let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
            && let Err(error) = input.repeat.configure(info, keyboard)
        {
            self.events.push_back(Event::Error(error));
        }
    }

    fn update_keymap(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        map: Keymap<'_>,
    ) {
        if let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
            && let Err(error) = input.repeat.keymap(map)
        {
            self.events.push_back(Event::Error(error));
        }
    }
}

delegate_dispatch2!(State);
