//! Toplevel icons through `xdg-toplevel-icon-v1`, where the compositor has it.
//!
//! The protocol takes square `wl_shm` buffers, so an icon is centered in a
//! transparent square and premultiplied. The compositor reads the buffer
//! when the icon is assigned; the icon object, buffer and pool are released
//! right after, as the protocol allows.

use crate::{State, Wayland};
use aegle_platform::{Error, Icon, Result, WindowId};
use smithay_client_toolkit::shm::slot::SlotPool;
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, globals::GlobalList, protocol::wl_shm,
};
use wayland_protocols::xdg::toplevel_icon::v1::client::{
    xdg_toplevel_icon_manager_v1::{self, XdgToplevelIconManagerV1},
    xdg_toplevel_icon_v1::XdgToplevelIconV1,
};

/// The icon manager, when the compositor offers one.
pub(crate) fn bind(
    globals: &GlobalList,
    qh: &QueueHandle<State>,
) -> Option<XdgToplevelIconManagerV1> {
    globals.bind(qh, 1..=1, ()).ok()
}

delegate_noop!(State: XdgToplevelIconV1);

impl Dispatch<XdgToplevelIconManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &XdgToplevelIconManagerV1,
        _: xdg_toplevel_icon_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Preferred sizes: the compositor scales the one image it is given.
    }
}

impl Wayland {
    pub(crate) fn set_icon(&mut self, id: WindowId, icon: Icon<'_>) -> Result<()> {
        let (native, _) = self.toplevel(id)?;
        if !icon.is_valid() {
            return Err(Error::InvalidSize);
        }
        let Some(manager) = &self.state.icons else {
            return Err(Error::Unsupported("window icons"));
        };
        let side = icon.width.max(icon.height);
        let (Ok(extent), Some(stride)) = (
            i32::try_from(side),
            side.checked_mul(4)
                .and_then(|stride| i32::try_from(stride).ok()),
        ) else {
            return Err(Error::InvalidSize);
        };
        let bytes = (stride as usize)
            .checked_mul(side as usize)
            .filter(|bytes| *bytes <= i32::MAX as usize)
            .ok_or(Error::InvalidSize)?;
        let mut pool = SlotPool::new(bytes, &self.state.shm).map_err(Error::backend)?;
        let (buffer, canvas) = pool
            .create_buffer(extent, extent, stride, wl_shm::Format::Argb8888)
            .map_err(Error::backend)?;
        canvas.fill(0);
        let (left, top) = (
            ((side - icon.width) / 2) as usize,
            ((side - icon.height) / 2) as usize,
        );
        let rows = canvas.chunks_exact_mut(stride as usize).skip(top);
        for (row, source) in rows.zip(icon.rgba.chunks_exact(icon.width as usize * 4)) {
            let row = &mut row[left * 4..][..source.len()];
            let pixels = row.as_chunks_mut::<4>().0.iter_mut();
            for (pixel, &[r, g, b, a]) in pixels.zip(source.as_chunks::<4>().0) {
                let premultiplied =
                    |channel: u8| ((u16::from(channel) * u16::from(a) + 127) / 255) as u8;
                // ARGB8888 is a native-endian word per pixel.
                *pixel =
                    u32::from_be_bytes([a, premultiplied(r), premultiplied(g), premultiplied(b)])
                        .to_ne_bytes();
            }
        }
        let object = manager.create_icon(&self.qh, ());
        object.add_buffer(buffer.wl_buffer(), 1);
        manager.set_icon(native.xdg_toplevel(), Some(&object));
        object.destroy();
        Ok(())
    }
}
