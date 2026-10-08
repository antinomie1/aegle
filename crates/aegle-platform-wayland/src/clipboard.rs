//! UTF-8 clipboard selections through `wl_data_device`, without blocking.
//! Drag and drop shares the data devices and sources; see `drag`.
//!
//! Each seat owns one data device. Writing retains one shared copy of the text
//! until another client replaces the selection; every paste request streams it
//! through the event loop. Reading collects one bounded pipe the same way.

use std::{
    io::{ErrorKind, Read, Write},
    rc::Rc,
};

use smithay_client_toolkit::{
    data_device_manager::{
        DataDeviceManagerState, ReadPipe, WritePipe,
        data_device::DataDevice,
        data_source::{CopyPasteSource, DataSourceHandler, DragSource},
    },
    reexports::calloop::PostAction,
};
use wayland_client::{
    Connection, QueueHandle,
    globals::GlobalList,
    protocol::{wl_data_device_manager::DndAction, wl_data_source::WlDataSource, wl_seat::WlSeat},
};

use crate::{Error, Event, State, Wayland, WindowId};

/// Offered and accepted text types, in preference order.
pub(crate) const MIME: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// Largest clipboard value read into memory, in bytes.
pub const CLIPBOARD_LIMIT: usize = 4 << 20;
/// Bytes moved per readiness; a page never blocks a ready pipe.
pub(crate) const CHUNK: usize = 4096;

pub(crate) struct ClipboardState {
    pub(crate) manager: Option<DataDeviceManagerState>,
    pub(crate) devices: Vec<Device>,
    source: Option<(CopyPasteSource, Rc<[u8]>)>,
    /// The drag this client started and its data.
    pub(crate) drag: Option<(DragSource, Rc<[u8]>)>,
}

/// A seat's data device.
pub(crate) struct Device {
    pub(crate) device: DataDevice,
    /// Latest key or button serial, which authorizes selections and drags.
    pub(crate) serial: u32,
    /// The window a drag is over, and whether the host accepted it.
    pub(crate) over: Option<(WindowId, bool)>,
}

impl ClipboardState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Self {
        Self {
            manager: DataDeviceManagerState::bind(globals, qh).ok(),
            devices: Vec::new(),
            source: None,
            drag: None,
        }
    }

    pub(crate) fn add_seat(&mut self, qh: &QueueHandle<State>, seat: &WlSeat) {
        if let Some(manager) = &self.manager {
            self.devices.push(Device {
                device: manager.get_data_device(qh, seat),
                serial: 0,
                over: None,
            });
        }
    }

    pub(crate) fn remove_seat(&mut self, seat: &WlSeat) {
        self.devices.retain(|d| d.device.data().seat() != seat);
    }

    /// Records the input serial that authorizes a later selection change.
    pub(crate) fn input(&mut self, seat: &WlSeat, serial: u32) {
        if let Some(device) = self.device(seat) {
            device.serial = serial;
        }
    }

    pub(crate) fn device(&mut self, seat: &WlSeat) -> Option<&mut Device> {
        self.devices
            .iter_mut()
            .find(|d| d.device.data().seat() == seat)
    }
}

impl Wayland {
    /// Offers UTF-8 text as `seat`'s clipboard, authorized by its latest key or
    /// button press. The text is retained until another client takes the
    /// selection and is written without blocking whenever a client pastes.
    pub fn set_clipboard(&mut self, seat: &WlSeat, text: &str) -> Result<(), Error> {
        let clipboard = &mut self.state.clipboard;
        let manager = clipboard
            .manager
            .as_ref()
            .ok_or_else(|| Error::backend("wl_data_device_manager is unavailable"))?;
        let source = manager.create_copy_paste_source(&self.qh, MIME);
        let device = clipboard
            .device(seat)
            .ok_or_else(|| Error::backend("seat has no data device"))?;
        source.set_selection(&device.device, device.serial);
        clipboard.source = Some((source, text.as_bytes().into()));
        Ok(())
    }

    /// Requests `seat`'s clipboard as UTF-8 text for `window`. The result
    /// arrives as [`Event::Clipboard`]. An empty selection, one without a text
    /// type, invalid UTF-8 or more than [`CLIPBOARD_LIMIT`] bytes delivers nothing.
    pub fn request_clipboard(&mut self, window: WindowId, seat: &WlSeat) -> Result<(), Error> {
        let Some(offer) = self
            .state
            .clipboard
            .device(seat)
            .and_then(|d| d.device.data().selection_offer())
        else {
            return Ok(());
        };
        let Some(mime) = offer.with_mime_types(|types| {
            MIME.into_iter()
                .find(|mime| types.iter().any(|offered| offered == mime))
        }) else {
            return Ok(());
        };
        let pipe = offer.receive(mime.into()).map_err(Error::backend)?;
        let seat = seat.clone();
        self.state.read_pipe(pipe, move |state, data| {
            if let Some(text) = data.and_then(|data| String::from_utf8(data).ok()) {
                state
                    .events
                    .push_back(Event::Clipboard { window, seat, text });
            }
        })
    }
}

impl State {
    /// Collects `pipe` without blocking, then calls `done` with its bytes, or
    /// `None` after an error or more than [`CLIPBOARD_LIMIT`] bytes.
    pub(crate) fn read_pipe(
        &self,
        pipe: ReadPipe,
        done: impl FnOnce(&mut State, Option<Vec<u8>>) + 'static,
    ) -> Result<(), Error> {
        let mut data = Vec::new();
        let mut done = Some(done);
        self.loop_handle
            .insert_source(pipe, move |_, file, state| {
                let start = data.len();
                data.resize(start + CHUNK, 0);
                let read = (&**file).read(&mut data[start..]);
                data.truncate(start + read.as_ref().map_or(0, |&read| read));
                let result = match read {
                    Ok(0) => Some(std::mem::take(&mut data)),
                    Ok(_) if data.len() <= CLIPBOARD_LIMIT => return PostAction::Continue,
                    Err(error) if error.kind() == ErrorKind::Interrupted => {
                        return PostAction::Continue;
                    }
                    _ => None,
                };
                if let Some(done) = done.take() {
                    done(state, result);
                }
                PostAction::Remove
            })
            .map_err(|error| Error::backend(error.error))?;
        Ok(())
    }

    pub(crate) fn write_selection(&mut self, pipe: WritePipe, data: Rc<[u8]>) {
        let mut offset = 0;
        let inserted = self.loop_handle.insert_source(pipe, move |_, file, _| {
            let end = data.len().min(offset + CHUNK);
            match (&**file).write(&data[offset..end]) {
                Ok(written) if offset + written < data.len() => {
                    offset += written;
                    PostAction::Continue
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => PostAction::Continue,
                _ => PostAction::Remove,
            }
        });
        if let Err(error) = inserted {
            self.events
                .push_back(Event::Error(Error::backend(error.error)));
        }
    }
}

impl DataSourceHandler for State {
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        _: String,
        pipe: WritePipe,
    ) {
        let clipboard = &self.clipboard;
        let copy = clipboard.source.as_ref().map(|(s, data)| (s.inner(), data));
        let drag = clipboard.drag.as_ref().map(|(s, data)| (s.inner(), data));
        let data = [copy, drag]
            .into_iter()
            .flatten()
            .find(|(s, _)| *s == source)
            .map(|(_, data)| data.clone());
        if let Some(data) = data {
            self.write_selection(pipe, data);
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.end_drag(source);
        if self
            .clipboard
            .source
            .as_ref()
            .is_some_and(|(current, _)| current.inner() == source)
        {
            self.clipboard.source = None;
        }
    }

    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.end_drag(source);
    }
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}
