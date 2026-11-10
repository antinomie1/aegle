//! Undecorated toplevels and window areas.
//!
//! Decorations are an `xdg-decoration` mode request: an undecorated window
//! asks for client-side decorations and draws none. Wherever the compositor
//! leaves a resizable toplevel without server-side decorations, the platform
//! gives it the contract's resize border. A primary press in the border or
//! in a move or resize area becomes `xdg_toplevel.move` or `.resize` with
//! the press's serial, and the compositor takes the pointer from there.

use crate::{
    Wayland,
    state::{Shell, WindowState},
};
use aegle_platform::{
    Point, Rect, ResizeEdge, Result, WindowArea, WindowId, WindowMode, area_at, border_edge,
    check_areas,
};
use smithay_client_toolkit::{seat::pointer::CursorIcon, shell::xdg::window::DecorationMode};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_protocols::xdg::shell::client::xdg_toplevel;

/// The cursor shown over what resizes by `edge`.
pub(crate) fn resize_icon(edge: ResizeEdge) -> CursorIcon {
    match edge {
        ResizeEdge::Top => CursorIcon::NResize,
        ResizeEdge::Bottom => CursorIcon::SResize,
        ResizeEdge::Left => CursorIcon::WResize,
        ResizeEdge::Right => CursorIcon::EResize,
        ResizeEdge::TopLeft => CursorIcon::NwResize,
        ResizeEdge::TopRight => CursorIcon::NeResize,
        ResizeEdge::BottomLeft => CursorIcon::SwResize,
        ResizeEdge::BottomRight => CursorIcon::SeResize,
    }
}

fn protocol_edge(edge: ResizeEdge) -> xdg_toplevel::ResizeEdge {
    match edge {
        ResizeEdge::Top => xdg_toplevel::ResizeEdge::Top,
        ResizeEdge::Bottom => xdg_toplevel::ResizeEdge::Bottom,
        ResizeEdge::Left => xdg_toplevel::ResizeEdge::Left,
        ResizeEdge::Right => xdg_toplevel::ResizeEdge::Right,
        ResizeEdge::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
        ResizeEdge::TopRight => xdg_toplevel::ResizeEdge::TopRight,
        ResizeEdge::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
    }
}

impl WindowState {
    /// The area at `point` that acts now: none on layers, popups and
    /// full-screen windows, and resizing only on a resizable toplevel in its
    /// normal mode, first by the border of one without server decorations.
    pub(crate) fn acting_area(&self, point: Point) -> WindowArea {
        let mode = self.info.mode;
        if !matches!(self.window, Shell::Xdg(_)) || mode == WindowMode::Fullscreen {
            return WindowArea::Client;
        }
        let resizes = self.resizable && mode == WindowMode::Normal;
        if resizes
            && !self.server_decorated
            && let Some(edge) = border_edge(self.info.size, point)
        {
            return WindowArea::Resize(edge);
        }
        match area_at(&self.areas, point) {
            WindowArea::Resize(_) if !resizes => WindowArea::Client,
            area => area,
        }
    }

    /// Begins the move or resize a primary press at `point` asks for, citing
    /// the press's `serial`. Returns whether it took the press, which is
    /// then not reported.
    pub(crate) fn press_area(&self, point: Point, seat: &WlSeat, serial: u32) -> bool {
        let Shell::Xdg(native) = &self.window else {
            return false;
        };
        match self.acting_area(point) {
            WindowArea::Move => native.move_(seat, serial),
            WindowArea::Resize(edge) => native.resize(seat, serial, protocol_edge(edge)),
            _ => return false,
        }
        true
    }
}

impl Wayland {
    pub(crate) fn set_decorations(&mut self, id: WindowId, decorated: bool) -> Result<()> {
        let (native, _) = self.toplevel(id)?;
        // Compositors without xdg-decoration never decorate; the request
        // then has nowhere to go.
        native.request_decoration_mode(Some(match decorated {
            true => DecorationMode::Server,
            false => DecorationMode::Client,
        }));
        Ok(())
    }

    pub(crate) fn set_areas(&mut self, id: WindowId, areas: &[(Rect, WindowArea)]) -> Result<()> {
        check_areas(areas)?;
        let (_, window) = self.toplevel(id)?;
        window.areas.clear();
        window.areas.extend_from_slice(areas);
        Ok(())
    }
}
