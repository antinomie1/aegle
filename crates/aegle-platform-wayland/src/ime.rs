//! One text-input-v3 object per seat, sharing the connection's event queue.

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, globals::GlobalList, protocol::wl_seat,
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};

use crate::{Error, Event, ImeEvent, ImeRequest, ImeUpdate, Preedit, State, WindowId};

struct Desired {
    window: WindowId,
    request: ImeRequest,
    rect: [i32; 4],
}

#[derive(Default)]
struct Pending {
    commit: Option<String>,
    preedit: Preedit,
    before: u32,
    after: u32,
    error: Option<&'static str>,
}

struct SeatInput {
    seat: wl_seat::WlSeat,
    object: ZwpTextInputV3,
    focus: Option<WindowId>,
    server_enabled: bool,
    content: Option<(
        zwp_text_input_v3::ContentHint,
        zwp_text_input_v3::ContentPurpose,
    )>,
    serial: u32,
    enable_serial: u32,
    blocked: bool,
    dirty: bool,
    pending: Pending,
}

impl SeatInput {
    fn commit(&mut self) {
        self.object.commit();
        self.serial = self.serial.wrapping_add(1);
    }

    fn disable(&mut self) {
        if std::mem::take(&mut self.server_enabled) {
            self.object.disable();
            self.commit();
        }
        self.content = None;
        self.blocked = false;
        self.pending = Pending::default();
    }

    fn send(&mut self, desired: Option<&Desired>) {
        self.dirty = false;
        let Some(Desired { request, rect, .. }) = desired else {
            self.disable();
            return;
        };
        let content = (request.hints, request.purpose);
        if self.server_enabled && self.content != Some(content) {
            // Leave invalidates local content, but does not necessarily disable
            // the server object. Reset that session after the next enter, when
            // requests are valid again. Changed content types also need enable.
            self.disable();
        }
        if self.content.is_none() {
            self.object.enable();
            self.enable_serial = self.serial.wrapping_add(1);
            self.server_enabled = true;
            self.content = Some(content);
            self.pending = Pending::default();
        }
        self.object.set_content_type(request.hints, request.purpose);
        self.object.set_text_change_cause(request.cause);
        self.object.set_surrounding_text(
            request.surrounding.clone(),
            request.cursor as i32,
            request.anchor as i32,
        );
        self.object
            .set_cursor_rectangle(rect[0], rect[1], rect[2], rect[3]);
        self.commit();
    }

    fn leave(&mut self) -> Option<WindowId> {
        // Requests after leave are ignored by the compositor. Invalidate local
        // state only; the next enter must enable and resend every supported field.
        self.content = None;
        self.pending = Pending::default();
        self.blocked = false;
        self.dirty = false;
        self.focus.take()
    }
}

pub(crate) struct ImeState {
    manager: Option<ZwpTextInputManagerV3>,
    seats: Vec<SeatInput>,
    desired: Vec<Desired>,
}

impl ImeState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Self {
        Self {
            manager: globals.bind(qh, 1..=1, ()).ok(),
            seats: Vec::new(),
            desired: Vec::new(),
        }
    }

    pub(crate) fn available(&self) -> bool {
        self.manager.is_some()
    }

    pub(crate) fn add_seat(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<State>) {
        if let Some(manager) = &self.manager {
            self.seats.push(SeatInput {
                seat: seat.clone(),
                object: manager.get_text_input(seat, qh, ()),
                focus: None,
                server_enabled: false,
                content: None,
                serial: 0,
                enable_serial: 0,
                blocked: false,
                dirty: false,
                pending: Pending::default(),
            });
        }
    }

    pub(crate) fn remove_seat(&mut self, seat: &wl_seat::WlSeat) -> Option<WindowId> {
        let index = self.seats.iter().position(|input| &input.seat == seat)?;
        let input = self.seats.swap_remove(index);
        input.object.destroy();
        input.focus
    }

    pub(crate) fn remove_window(&mut self, window: WindowId) {
        self.desired.retain(|desired| desired.window != window);
        for input in &mut self.seats {
            if input.focus == Some(window) {
                input.disable();
                input.leave();
            }
        }
    }

    pub(crate) fn configure(
        &mut self,
        window: WindowId,
        request: Option<ImeRequest>,
    ) -> Result<(), Error> {
        let next = request
            .map(|request| {
                let rect = request.validate()?;
                if !self.available() {
                    return Err(Error::ImeUnavailable);
                }
                Ok(Desired {
                    window,
                    request,
                    rect,
                })
            })
            .transpose()?;
        let index = self
            .desired
            .iter()
            .position(|desired| desired.window == window);
        let changed = match (index, next) {
            (Some(index), Some(next)) => {
                let changed = self.desired[index].request != next.request;
                self.desired[index] = next;
                changed
            }
            (Some(index), None) => {
                self.desired.swap_remove(index);
                true
            }
            (None, Some(next)) => {
                self.desired.push(next);
                true
            }
            (None, None) => false,
        };
        let desired = self.desired.iter().find(|desired| desired.window == window);
        for input in &mut self.seats {
            if input.focus == Some(window) {
                input.dirty |= changed;
                // Cancellation ends the session immediately. Only state updates
                // inside the current session wait for matching acknowledgements.
                if input.dirty && (!input.blocked || desired.is_none()) {
                    input.send(desired);
                }
            }
        }
        Ok(())
    }
}

impl Drop for ImeState {
    fn drop(&mut self) {
        for input in &self.seats {
            input.object.destroy();
        }
        if let Some(manager) = &self.manager {
            manager.destroy();
        }
    }
}

impl Dispatch<ZwpTextInputV3, ()> for State {
    fn event(
        state: &mut Self,
        object: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let window = match &event {
            zwp_text_input_v3::Event::Enter { surface } => state.window_id(surface),
            _ => None,
        };
        let Some(input) = state
            .ime
            .seats
            .iter_mut()
            .find(|input| &input.object == object)
        else {
            return;
        };
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                input.leave();
                input.focus = window;
                if let Some(window) = window {
                    input.send(
                        state
                            .ime
                            .desired
                            .iter()
                            .find(|desired| desired.window == window),
                    );
                    state.events.push_back(Event::Ime {
                        window,
                        seat: input.seat.clone(),
                        event: ImeEvent::Entered,
                    });
                }
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                if let Some(window) = input.leave() {
                    state.events.push_back(Event::Ime {
                        window,
                        seat: input.seat.clone(),
                        event: ImeEvent::Left,
                    });
                }
            }
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => match Preedit::from_protocol(text, cursor_begin, cursor_end) {
                Ok(preedit) => input.pending.preedit = preedit,
                Err(error) => input.pending.error = Some(error),
            },
            zwp_text_input_v3::Event::CommitString { text } => input.pending.commit = text,
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                input.pending.before = before_length;
                input.pending.after = after_length;
            }
            zwp_text_input_v3::Event::Done { serial } => {
                let pending = std::mem::take(&mut input.pending);
                let current = serial == input.serial;
                // Already queued acknowledgements can arrive after disable.
                // They belong to the canceled editor, not the next session.
                // Unsigned distances keep the interval [enable, latest commit]
                // valid when the protocol's u32 commit counter wraps.
                if input.content.is_none()
                    || input.focus.is_none()
                    || serial.wrapping_sub(input.enable_serial)
                        > input.serial.wrapping_sub(input.enable_serial)
                {
                    return;
                }
                input.blocked = !current;
                if let Some(error) = pending.error {
                    state
                        .events
                        .push_back(Event::Error(Error::InvalidIme(error)));
                } else if let Some(window) = input.focus.filter(|_| input.content.is_some()) {
                    state.events.push_back(Event::Ime {
                        window,
                        seat: input.seat.clone(),
                        event: ImeEvent::Update(ImeUpdate {
                            serial,
                            current,
                            commit: pending.commit,
                            preedit: pending.preedit,
                            delete_before: pending.before,
                            delete_after: pending.after,
                        }),
                    });
                }
                // Do not send cached surrounding here: the host must first apply
                // this transaction and configure with the resulting editor state.
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore ZwpTextInputManagerV3);
