//! Drag and drop through `wl_data_device`: windows as drop targets for text
//! and files, and copy drags this client starts.
//!
//! The host answers each [`Event::Drag`] with [`Wayland::accept_drag`];
//! the compositor drops only onto a window that accepted. Dropped data is
//! read through the event loop like a paste, then the offer is finished.

use smithay_client_toolkit::data_device_manager::{
    data_device::DataDeviceHandler,
    data_offer::{DataOfferHandler, DragOffer},
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_surface::WlSurface,
    },
};

use aegle_types::{DragData, Point};

use crate::{Error, Event, State, Wayland, WindowId, WlSeat, clipboard::MIME};

/// The file list type; preferred over text when both are offered.
const URI_LIST: &str = "text/uri-list";

/// The type to read from `offer`: files first, then text.
fn choose(offer: &DragOffer) -> Option<&'static str> {
    offer.with_mime_types(|types| {
        [URI_LIST]
            .into_iter()
            .chain(MIME)
            .find(|mime| types.iter().any(|offered| offered == mime))
    })
}

/// Decodes dropped bytes of type `mime`; a file list with no local files
/// is nothing.
fn decode(mime: &str, data: Vec<u8>) -> Option<DragData> {
    let text = String::from_utf8(data).ok()?;
    if mime != URI_LIST {
        return Some(DragData::Text(text));
    }
    let files: Vec<_> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| aegle_types::path_from_file_uri(line.trim()))
        .collect();
    (!files.is_empty()).then_some(DragData::Files(files))
}

impl Wayland {
    /// Accepts or refuses the drag over a window of `seat`, after each
    /// [`Event::Drag`]. Only offers of text or files can be accepted; the
    /// accepted action is always copy.
    pub fn accept_drag(&mut self, seat: &WlSeat, accept: bool) {
        let Some(device) = self.state.clipboard.device(seat) else {
            return;
        };
        let Some((_, accepted)) = &mut device.over else {
            return;
        };
        let Some(offer) = device.device.data().drag_offer() else {
            return;
        };
        let mime = choose(&offer).filter(|_| accept);
        if *accepted == mime.is_some() {
            return;
        }
        *accepted = mime.is_some();
        offer.accept_mime_type(offer.serial, mime.map(String::from));
        let action = if mime.is_some() {
            DndAction::Copy
        } else {
            DndAction::empty()
        };
        offer.set_actions(action, action);
    }

    /// Starts a copy drag of `data` from `window`, authorized by `seat`'s
    /// latest button press, which must still be held. The data is served to
    /// the target without blocking; the drag ends when it is dropped or
    /// cancelled.
    pub fn start_drag(
        &mut self,
        window: WindowId,
        seat: &WlSeat,
        data: &DragData,
    ) -> Result<(), Error> {
        let surface = self
            .state
            .windows
            .iter()
            .find(|w| w.id == window)
            .ok_or(Error::InvalidWindow)?
            .window
            .wl_surface()
            .clone();
        let clipboard = &mut self.state.clipboard;
        let manager = clipboard
            .manager
            .as_ref()
            .ok_or_else(|| Error::backend("wl_data_device_manager is unavailable"))?;
        let (mimes, bytes): (&[&str], _) = match data {
            DragData::Text(text) => (&MIME, text.as_bytes().into()),
            DragData::Files(files) => {
                let mut list = String::new();
                for file in files {
                    list.push_str(&aegle_types::file_uri(file));
                    list.push_str("\r\n");
                }
                (&[URI_LIST], list.into_bytes().into())
            }
        };
        let source = manager.create_drag_and_drop_source(&self.qh, mimes, DndAction::Copy);
        let device = clipboard
            .device(seat)
            .ok_or_else(|| Error::backend("seat has no data device"))?;
        source.start_drag(&device.device, &surface, None, device.serial);
        clipboard.drag = Some((source, bytes));
        Ok(())
    }
}

impl State {
    /// Forgets this client's drag once `source` is finished or cancelled.
    pub(crate) fn end_drag(&mut self, source: &WlDataSource) {
        if self
            .clipboard
            .drag
            .as_ref()
            .is_some_and(|(drag, _)| drag.inner() == source)
        {
            self.clipboard.drag = None;
        }
    }

    fn device_of(&mut self, device: &WlDataDevice) -> Option<&mut crate::clipboard::Device> {
        self.clipboard
            .devices
            .iter_mut()
            .find(|d| d.device.inner() == device)
    }

    fn drag_event(&mut self, device: &WlDataDevice, x: f64, y: f64) {
        let Some(d) = self.device_of(device) else {
            return;
        };
        if let Some((window, _)) = d.over {
            let seat = d.device.data().seat().clone();
            let position = Point::new(x as f32, y as f32);
            self.events.push_back(Event::Drag {
                window,
                seat,
                position,
            });
        }
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        let window = self.window_id(surface);
        if let Some(d) = self.device_of(device) {
            d.over = window.map(|window| (window, false));
        }
        self.drag_event(device, x, y);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(d) = self.device_of(device) else {
            return;
        };
        if let Some((window, _)) = d.over.take() {
            let seat = d.device.data().seat().clone();
            self.events.push_back(Event::DragLeave { window, seat });
        }
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        self.drag_event(device, x, y);
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(d) = self.device_of(device) else {
            return;
        };
        let (Some((window, accepted)), Some(offer)) = (d.over.take(), d.device.data().drag_offer())
        else {
            return;
        };
        let seat = d.device.data().seat().clone();
        let mime = choose(&offer).filter(|_| accepted);
        let pipe = mime.map(|mime| offer.receive(mime.into()));
        let Some(Ok(pipe)) = pipe else {
            self.events.push_back(Event::DragLeave { window, seat });
            return;
        };
        let (mime, position) = (mime.unwrap(), Point::new(offer.x as f32, offer.y as f32));
        let offer = offer.inner().clone();
        let read = self.read_pipe(pipe, move |state, data| {
            let event = match data.and_then(|data| decode(mime, data)) {
                Some(data) => Event::Drop {
                    window,
                    seat,
                    position,
                    data,
                },
                None => Event::DragLeave { window, seat },
            };
            state.events.push_back(event);
            if offer.version() >= 3 {
                offer.finish();
            }
            offer.destroy();
        });
        if let Err(error) = read {
            self.events.push_back(Event::Error(error));
        }
    }
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
