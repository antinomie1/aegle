//! UTF-8 clipboard selections through `wl_data_device`, without blocking.
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
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::{CopyPasteSource, DataSourceHandler},
    },
    reexports::calloop::PostAction,
};
use wayland_client::{
    Connection, QueueHandle,
    globals::GlobalList,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_seat::WlSeat, wl_surface::WlSurface,
    },
};

use crate::{Error, Event, State, Wayland, WindowId};

/// Offered and accepted text types, in preference order.
const MIME: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// Largest clipboard value read into memory, in bytes.
pub const CLIPBOARD_LIMIT: usize = 4 << 20;
/// Bytes moved per readiness; a page never blocks a ready pipe.
const CHUNK: usize = 4096;

pub(crate) struct ClipboardState {
    manager: Option<DataDeviceManagerState>,
    /// Data device and latest key/button serial for each seat.
    devices: Vec<(DataDevice, u32)>,
    source: Option<(CopyPasteSource, Rc<[u8]>)>,
}

impl ClipboardState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Self {
        Self {
            manager: DataDeviceManagerState::bind(globals, qh).ok(),
            devices: Vec::new(),
            source: None,
        }
    }

    pub(crate) fn add_seat(&mut self, qh: &QueueHandle<State>, seat: &WlSeat) {
        if let Some(manager) = &self.manager {
            self.devices.push((manager.get_data_device(qh, seat), 0));
        }
    }

    pub(crate) fn remove_seat(&mut self, seat: &WlSeat) {
        self.devices
            .retain(|(device, _)| device.data().seat() != seat);
    }

    /// Records the input serial that authorizes a later selection change.
    pub(crate) fn input(&mut self, seat: &WlSeat, serial: u32) {
        if let Some((_, latest)) = self.device(seat) {
            *latest = serial;
        }
    }

    fn device(&mut self, seat: &WlSeat) -> Option<&mut (DataDevice, u32)> {
        self.devices
            .iter_mut()
            .find(|(device, _)| device.data().seat() == seat)
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
        let (device, serial) = clipboard
            .device(seat)
            .ok_or_else(|| Error::backend("seat has no data device"))?;
        source.set_selection(device, *serial);
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
            .and_then(|(device, _)| device.data().selection_offer())
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
        let mut data = Vec::new();
        self.state
            .loop_handle
            .insert_source(pipe, move |_, file, state| {
                let start = data.len();
                data.resize(start + CHUNK, 0);
                let read = (&**file).read(&mut data[start..]);
                data.truncate(start + read.as_ref().map_or(0, |&read| read));
                match read {
                    Ok(0) => {
                        if let Ok(text) = String::from_utf8(std::mem::take(&mut data)) {
                            state.events.push_back(Event::Clipboard {
                                window,
                                seat: seat.clone(),
                                text,
                            });
                        }
                        PostAction::Remove
                    }
                    Ok(_) if data.len() <= CLIPBOARD_LIMIT => PostAction::Continue,
                    Err(error) if error.kind() == ErrorKind::Interrupted => PostAction::Continue,
                    _ => PostAction::Remove,
                }
            })
            .map_err(|error| Error::backend(error.error))?;
        Ok(())
    }
}

impl State {
    fn write_selection(&mut self, pipe: WritePipe, data: Rc<[u8]>) {
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
        if let Some((current, data)) = &self.clipboard.source
            && current.inner() == source
        {
            let data = data.clone();
            self.write_selection(pipe, data);
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
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
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

// Drag and drop is not offered; selection offers are read on request.
impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &WlSurface,
    ) {
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataOfferHandler for State {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}
