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
        wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
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
    Event, WindowId, WindowInfo, buffers::SoftwareBuffers, clipboard::ClipboardState,
    ime::ImeState, input::InputState,
};

/// Surface role: an ordinary toplevel or a wlr layer surface.
#[derive(Clone, Debug)]
pub(crate) enum Shell {
    Xdg(Window),
    Layer(LayerSurface),
}

impl Shell {
    pub(crate) fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Self::Xdg(window) => window.wl_surface(),
            Self::Layer(layer) => layer.wl_surface(),
        }
    }
    pub(crate) fn commit(&self) {
        self.wl_surface().commit();
    }
}

pub(crate) struct WindowState {
    pub(crate) id: WindowId,
    pub(crate) window: Shell,
    pub(crate) info: WindowInfo,
    pub(crate) buffers: SoftwareBuffers,
    pub(crate) dirty: bool,
    pub(crate) redraw_queued: bool,
    pub(crate) frame_pending: bool,
    // An external acquisition may fail before its pending surface state commits.
    pub(crate) frame_requested: bool,
    /// Shape applied whenever a pointer enters the window.
    pub(crate) cursor: aegle_types::Cursor,
}

pub(crate) struct State {
    pub(crate) registry_state: RegistryState,
    pub(crate) seat_state: SeatState,
    pub(crate) output_state: OutputState,
    pub(crate) compositor: CompositorState,
    pub(crate) shell: XdgShell,
    pub(crate) layer_shell: Option<LayerShell>,
    pub(crate) shm: Shm,
    pub(crate) loop_handle: LoopHandle<'static, State>,
    pub(crate) input: InputState,
    pub(crate) ime: ImeState,
    pub(crate) clipboard: ClipboardState,
    pub(crate) windows: Vec<WindowState>,
    pub(crate) events: VecDeque<Event>,
    pub(crate) preferences: crate::Preferences,
}

impl State {
    pub(crate) fn window_id(&self, surface: &wl_surface::WlSurface) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|w| w.window.wl_surface() == surface)
            .map(|w| w.id)
    }

    /// Applies a configure; `None` keeps the current or preferred extent.
    fn configured(
        &mut self,
        surface: &wl_surface::WlSurface,
        size: (Option<u32>, Option<u32>),
        active: bool,
    ) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.window.wl_surface() == surface)
        {
            window.info.size.width = size.0.unwrap_or(window.info.size.width);
            window.info.size.height = size.1.unwrap_or(window.info.size.height);
            window.info.active = active;
            window.info.configured = true;
            window.dirty = true;
            self.events.push_back(Event::Configure {
                window: window.id,
                info: window.info,
            });
        }
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
        let size = (
            configure.new_size.0.map(|v| v.get()),
            configure.new_size.1.map(|v| v.get()),
        );
        self.configured(native.wl_surface(), size, configure.is_activated());
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(window) = self.window_id(layer.wl_surface()) {
            self.events.push_back(Event::Close { window });
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        // Zero leaves that axis to the client.
        let (width, height) = configure.new_size;
        let size = (
            (width != 0).then_some(width),
            (height != 0).then_some(height),
        );
        self.configured(layer.wl_surface(), size, true);
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
