use std::{collections::VecDeque, io, os::fd::AsFd, time::Duration};

use smithay_client_toolkit::{
    compositor::{CompositorState, FrameCallbackData},
    output::OutputState,
    reexports::{
        calloop::{
            EventLoop, Interest, Mode, PostAction, RegistrationToken, generic::Generic, ping,
        },
        calloop_wayland_source::WaylandSource,
    },
    registry::RegistryState,
    seat::SeatState,
    shell::{
        wlr_layer::{Anchor, LayerShell},
        xdg::{XdgShell, window::WindowDecorations},
    },
    shm::Shm,
};
use wayland_client::{
    Connection, Proxy, QueueHandle, backend::WaylandError, globals::registry_queue_init,
};

use crate::{
    Error, Event, ImeRequest, PixelSize, PresentError, State, WindowId, WindowInfo, WindowOptions,
    buffers::SoftwareBuffers,
    clipboard::ClipboardState,
    ime::ImeState,
    input::InputState,
    state::{Shell, WindowState},
};
use aegle_types::{Cursor, PixelRect, Region};

/// One Wayland connection and blocking event loop, shared by all its windows.
///
/// This is a UI-thread owner. Software frames are allocated lazily and are never
/// rendered periodically: a host mutation must explicitly request a redraw.
pub struct Wayland {
    event_loop: EventLoop<'static, State>,
    pub(crate) state: State,
    pub(crate) connection: Connection,
    pub(crate) qh: QueueHandle<State>,
    next_id: u64,
    source: RegistrationToken,
    wake: Option<(WakeHandle, RegistrationToken)>,
}

/// Cloneable cross-thread signal for a host work queue, without a polling timer.
/// A handle keeps only its signal resource alive, never the window or UI state.
#[derive(Clone, Debug)]
pub struct WakeHandle(ping::Ping);

impl WakeHandle {
    /// Requests [`Event::Wake`]. Store work in the host queue before calling.
    /// Requests after the backend is destroyed cannot deliver an event.
    pub fn wake(&self) {
        self.0.ping();
    }
}

impl Wayland {
    /// Connects to `WAYLAND_DISPLAY` and binds required xdg-shell/SHM globals.
    /// text-input-v3 is optional; enabling it explicitly reports missing support.
    pub fn connect() -> Result<Self, Error> {
        let connection = Connection::connect_to_env().map_err(Error::backend)?;
        let (globals, queue) = registry_queue_init(&connection).map_err(Error::backend)?;
        let qh = queue.handle();
        let event_loop = EventLoop::try_new().map_err(Error::backend)?;
        let compositor = CompositorState::bind(&globals, &qh).map_err(Error::backend)?;
        if compositor.wl_compositor().version() < 4 {
            return Err(Error::backend("wl_compositor version 4 is required"));
        }
        let shell = XdgShell::bind(&globals, &qh).map_err(Error::backend)?;
        let layer_shell = LayerShell::bind(&globals, &qh).ok();
        let shm = Shm::bind(&globals, &qh).map_err(Error::backend)?;
        let seat_state = SeatState::new(&globals, &qh);
        let ime = ImeState::bind(&globals, &qh);
        let clipboard = ClipboardState::bind(&globals, &qh);
        let mut state = State {
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            seat_state,
            compositor,
            shell,
            layer_shell,
            shm,
            loop_handle: event_loop.handle(),
            input: InputState::default(),
            ime,
            clipboard,
            windows: Vec::new(),
            scale: crate::scale::ScaleGlobals::bind(&globals, &qh),
            events: VecDeque::new(),
            preferences: Default::default(),
        };
        state.init_input(&connection, &qh);
        if let Some((stream, mut portal)) = crate::portal::connect(&mut state.preferences) {
            let source = Generic::new(stream, Interest::READ, Mode::Level);
            event_loop
                .handle()
                .insert_source(source, move |_, stream, state: &mut State| {
                    let old = state.preferences;
                    let open = portal.read(stream, &mut state.preferences);
                    if state.preferences != old {
                        state
                            .events
                            .push_back(Event::Preferences(state.preferences));
                    }
                    Ok(if open {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                })
                .map_err(|error| Error::backend(error.error))?;
        }
        let source = WaylandSource::new(connection.clone(), queue)
            .insert(event_loop.handle())
            .map_err(Error::backend)?;
        Ok(Self {
            event_loop,
            state,
            connection,
            qh,
            next_id: 0,
            source,
            wake: None,
        })
    }

    /// Desktop portal appearance preferences known so far. Values read within a
    /// short startup bound are present after [`Self::connect`]; later replies and
    /// changes arrive as [`Event::Preferences`]. Unknown without a session bus.
    pub fn preferences(&self) -> crate::Preferences {
        self.state.preferences
    }

    /// Lazily creates one wake source for background work or accessibility.
    /// Subsequent calls share the source; unused backends allocate no signal FD.
    pub fn wake_handle(&mut self) -> Result<WakeHandle, Error> {
        if self.wake.is_none() {
            let (sender, source) = ping::make_ping().map_err(Error::backend)?;
            let token = self
                .event_loop
                .handle()
                .insert_source(source, |_, _, state| {
                    state.events.push_back(Event::Wake);
                })
                .map_err(Error::backend)?;
            self.wake = Some((WakeHandle(sender), token));
        }
        Ok(self.wake.as_ref().unwrap().0.clone())
    }

    /// Creates an ordinary toplevel, or a layer surface when requested, and
    /// requests its initial configure. No pixel memory is allocated until the
    /// first presentation. Layers require `zwlr_layer_shell_v1`.
    pub fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId, Error> {
        let info = WindowInfo {
            size: options.size,
            scale: 1.0,
            active: false,
            configured: false,
        };
        info.buffer_size()?;
        if [options.title, options.app_id]
            .iter()
            .any(|value| value.len() > 4000 || value.contains('\0'))
        {
            return Err(Error::InvalidString);
        }
        let id = WindowId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Error::backend("window identity exhausted"))?;
        let surface = self.state.compositor.create_surface(&self.qh);
        let fractional = self
            .state
            .scale
            .as_ref()
            .map(|scale| scale.attach(&surface, &self.qh));
        let window = if let Some(layer) = options.layer {
            let shell = self.state.layer_shell.as_ref().ok_or_else(|| {
                surface.destroy();
                Error::backend("zwlr_layer_shell_v1 is unavailable")
            })?;
            let native = shell.create_layer_surface(
                &self.qh,
                surface,
                layer.layer,
                Some(options.app_id),
                None,
            );
            let stretch = |edges: Anchor, extent: u32| {
                if layer.anchor.contains(edges) {
                    0
                } else {
                    extent
                }
            };
            native.set_size(
                stretch(Anchor::LEFT | Anchor::RIGHT, options.size.width),
                stretch(Anchor::TOP | Anchor::BOTTOM, options.size.height),
            );
            native.set_anchor(layer.anchor);
            native.set_exclusive_zone(layer.exclusive_zone);
            native.set_keyboard_interactivity(layer.keyboard);
            Shell::Layer(native)
        } else {
            let native =
                self.state
                    .shell
                    .create_window(surface, WindowDecorations::RequestServer, &self.qh);
            native.set_title(options.title);
            native.set_app_id(options.app_id);
            Shell::Xdg(native)
        };
        window.commit();
        self.state.windows.push(WindowState {
            id,
            window,
            info,
            buffers: SoftwareBuffers::new(options.buffer_budget),
            dirty: true,
            redraw_queued: false,
            frame_pending: false,
            frame_requested: false,
            cursor: Cursor::Default,
            fractional,
        });
        Ok(id)
    }

    /// Removes the window and its IME session. Its ID is never reused.
    pub fn remove_window(&mut self, id: WindowId) -> Result<(), Error> {
        let index = self
            .state
            .windows
            .iter()
            .position(|w| w.id == id)
            .ok_or(Error::InvalidWindow)?;
        self.state.ime.remove_window(id);
        self.state
            .remove_input_window(id, &self.connection, &self.qh);
        self.state.windows.swap_remove(index);
        self.state.events.retain(|event| match event {
            Event::Configure { window, .. }
            | Event::Redraw { window }
            | Event::Close { window }
            | Event::KeyboardFocus { window, .. }
            | Event::Key { window, .. }
            | Event::Modifiers { window, .. }
            | Event::Pointer { window, .. }
            | Event::Touch { window, .. }
            | Event::Ime { window, .. }
            | Event::Clipboard { window, .. } => *window != id,
            _ => true,
        });
        Ok(())
    }

    /// Waits for native events, blocking indefinitely for `None`.
    /// Returns immediately if application events are already queued. It does not
    /// introduce a periodic timer; keyboard repeat uses compositor settings.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), Error> {
        self.state.queue_redraws();
        // WaylandSource only polls READ and ignores a blocked flush. Add WRITE
        // readiness just while backpressured, so idle dispatch cannot deadlock.
        // A dup FD avoids registering the source's existing FD twice with epoll.
        let writable = match self.connection.flush() {
            Ok(()) => None,
            Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                let fd = self
                    .connection
                    .as_fd()
                    .try_clone_to_owned()
                    .map_err(Error::backend)?;
                Some(
                    self.event_loop
                        .handle()
                        .insert_source(Generic::new(fd, Interest::WRITE, Mode::Level), |_, _, _| {
                            Ok(PostAction::Remove)
                        })
                        .map_err(Error::backend)?,
                )
            }
            Err(error) => return Err(Error::backend(error)),
        };
        let timeout = if self.state.events.is_empty() {
            timeout
        } else {
            Some(Duration::ZERO)
        };
        let result = self.event_loop.dispatch(timeout, &mut self.state);
        if let Some(token) = writable {
            self.event_loop.handle().remove(token);
        }
        result.map_err(Error::backend)?;
        self.state.queue_redraws();
        Ok(())
    }

    /// Pops the oldest application event. Drain before blocking again.
    pub fn next_event(&mut self) -> Option<Event> {
        self.state.events.pop_front()
    }

    /// Marks a window dirty. Multiple changes coalesce behind the frame callback.
    pub fn request_redraw(&mut self, id: WindowId) -> Result<(), Error> {
        self.window_mut(id)?.dirty = true;
        self.state.queue_redraws();
        Ok(())
    }

    /// Sets the shape shown over a window. It applies immediately to every pointer
    /// inside the window and again whenever a pointer enters it, so the host only
    /// needs to call this when the desired shape changes.
    pub fn set_cursor(&mut self, id: WindowId, cursor: Cursor) -> Result<(), Error> {
        let window = self.window_mut(id)?;
        if std::mem::replace(&mut window.cursor, cursor) != cursor {
            self.state.apply_cursor(&self.connection, id);
        }
        Ok(())
    }

    /// Returns the latest geometry; it can change between redraw notifications.
    pub fn window_info(&self, id: WindowId) -> Result<WindowInfo, Error> {
        self.state
            .windows
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.info)
            .ok_or(Error::InvalidWindow)
    }

    /// Returns live SHM mapping bytes, including slot alignment.
    pub fn buffer_bytes(&self, id: WindowId) -> Result<usize, Error> {
        self.state
            .windows
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.buffers.allocated_bytes())
            .ok_or(Error::InvalidWindow)
    }

    /// Whether the compositor advertised text-input-v3 at connection time.
    pub fn ime_available(&self) -> bool {
        self.state.ime.available()
    }

    /// Sets or disables the focused editable control's native IME state.
    /// Process queued input batches before publishing updated surrounding text.
    pub fn configure_ime(
        &mut self,
        id: WindowId,
        request: Option<ImeRequest>,
    ) -> Result<(), Error> {
        self.window_info(id)?;
        let disabled = request.is_none();
        self.state.ime.configure(id, request)?;
        if disabled {
            // Native events can already be in the host queue when a pointer or
            // focus handler replaces the editor. They belong to the old session.
            self.state.events.retain(|event| {
                !matches!(event,
                Event::Ime { window, event: crate::ImeEvent::Update(_), .. } if *window == id)
            });
        }
        Ok(())
    }

    /// Draws directly into an idle SHM buffer as premultiplied sRGB BGRA8, the
    /// memory order of little-endian ARGB8888 (big-endian targets reorder it).
    ///
    /// `damage` is the area that changed since the last presented frame, in
    /// buffer pixels; `None` changes every pixel. `draw` must clear or
    /// overwrite every rectangle of the region it is given; the rest of the
    /// buffer already holds the previous frame, and only `damage` is reported
    /// to the compositor.
    /// Returns false without invoking `draw` if not yet configured, a frame is
    /// pending, or both buffers await release. A drawing error never presents
    /// partial pixels; request a redraw to retry.
    pub fn present<E>(
        &mut self,
        id: WindowId,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> Result<(), E>,
    ) -> Result<bool, PresentError<E>> {
        let window = self
            .state
            .windows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or(PresentError::Platform(Error::InvalidWindow))?;
        window.redraw_queued = false;
        if !window.info.configured || window.frame_pending {
            return Ok(false);
        }
        window.dirty = false;
        let size = window.info.buffer_size().map_err(PresentError::Platform)?;
        // Pending surface state only takes effect with the commit below.
        window.apply_scale();
        let Some(buffer) =
            window
                .buffers
                .paint(&self.state.shm, size, damage, |pixels, region| {
                    draw(pixels, size, region)
                })?
        else {
            window.dirty = true;
            return Ok(false);
        };
        let surface = window.window.wl_surface();
        let full = [PixelRect::full(size.width, size.height)];
        let damage = damage.as_ref().map_or(&full[..], Region::rects);
        for rect in damage {
            surface.damage_buffer(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            );
        }
        buffer
            .attach_to(surface)
            .map_err(|e| PresentError::Platform(Error::backend(e)))?;
        if !window.frame_requested {
            surface.frame(&self.qh, FrameCallbackData(surface.clone()));
        }
        window.frame_requested = false;
        window.window.commit();
        window.frame_pending = true;
        window.dirty = false;
        Ok(true)
    }

    fn window_mut(&mut self, id: WindowId) -> Result<&mut WindowState, Error> {
        self.state
            .windows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or(Error::InvalidWindow)
    }
}

impl Drop for Wayland {
    fn drop(&mut self) {
        self.state.shutdown_input(&self.connection, &self.qh);
        // Explicitly remove the source before field destruction: queued protocol
        // objects may hold loop handles, so merely dropping EventLoop is not enough.
        self.event_loop.handle().remove(self.source);
        if let Some((_, token)) = self.wake.take() {
            self.event_loop.handle().remove(token);
        }
    }
}
