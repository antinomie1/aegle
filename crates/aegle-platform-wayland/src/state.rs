use std::collections::VecDeque;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::calloop::LoopHandle,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::SeatState,
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{wl_output, wl_surface},
};

use crate::{
    Event, WindowId, WindowInfo, buffers::SoftwareBuffers, ime::ImeState, input::InputState,
};

pub(crate) struct WindowState {
    pub(crate) id: WindowId,
    pub(crate) window: Window,
    pub(crate) info: WindowInfo,
    pub(crate) buffers: SoftwareBuffers,
    pub(crate) dirty: bool,
    pub(crate) redraw_queued: bool,
    pub(crate) frame_pending: bool,
    // An external acquisition may fail before its pending surface state commits.
    pub(crate) frame_requested: bool,
}

pub(crate) struct State {
    pub(crate) registry_state: RegistryState,
    pub(crate) seat_state: SeatState,
    pub(crate) output_state: OutputState,
    pub(crate) compositor: CompositorState,
    pub(crate) shell: XdgShell,
    pub(crate) shm: Shm,
    pub(crate) loop_handle: LoopHandle<'static, State>,
    pub(crate) input: InputState,
    pub(crate) ime: ImeState,
    pub(crate) windows: Vec<WindowState>,
    pub(crate) events: VecDeque<Event>,
}

impl State {
    pub(crate) fn window_id(&self, surface: &wl_surface::WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|w| w.window.wl_surface() == surface)
            .map(|w| w.id)
    }

    pub(crate) fn queue_redraws(&mut self) {
        for window in &mut self.windows {
            if window.info.configured
                && window.dirty
                && !window.redraw_queued
                && !window.frame_pending
                && window.buffers.has_free()
            {
                window.redraw_queued = true;
                self.events.push_back(Event::Redraw { window: window.id });
            }
        }
    }
}

impl WindowHandler for State {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(window) = self.window_id(window.wl_surface()) {
            self.events.push_back(Event::Close { window });
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        native: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        if let Some(window) = self.windows.iter_mut().find(|w| w.window == *native) {
            window.info.size.width = configure
                .new_size
                .0
                .map_or(window.info.size.width, |v| v.get());
            window.info.size.height = configure
                .new_size
                .1
                .map_or(window.info.size.height, |v| v.get());
            window.info.active = configure.is_activated();
            window.info.configured = true;
            window.dirty = true;
            self.events.push_back(Event::Configure {
                window: window.id,
                info: window.info,
            });
        }
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        conn: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.window.wl_surface() == surface)
        {
            window.info.scale = factor as u32;
            window.dirty = true;
            self.events.push_back(Event::Configure {
                window: window.id,
                info: window.info,
            });
        }
        self.update_input_cursor_scale(conn, surface);
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.window.wl_surface() == surface)
        {
            window.frame_pending = false;
            window.frame_requested = false;
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(State);
