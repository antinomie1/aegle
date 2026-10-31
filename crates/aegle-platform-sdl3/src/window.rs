//! One SDL3 video connection and its windows, driven from the UI thread.
//!
//! SDL3 owns the display connection, so this backend keeps no protocol state of
//! its own: it registers one user event as its wake signal, translates
//! `SDL_EVENT_*` into contract events and lends the window's surface to the
//! software presenter. Software frames are paced by a deadline derived from the
//! display's refresh rate, because SDL3 reports no per-surface frame callback.
#![allow(unsafe_code)]

use std::{
    collections::VecDeque,
    ffi::{CStr, c_int},
    os::unix::net::UnixStream,
    ptr::NonNull,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use aegle_platform::{
    Cursor, DragData, DrawResult, Error, Event, ImeRequest, PixelRect, PixelSize, Preferences,
    Region, SeatId, WakeHandle, WindowId, WindowInfo, WindowOptions,
};
use aegle_types::Point;

use crate::sys;

/// The single seat of an SDL3 application: SDL reports one keyboard, pointer
/// and touch group with one focus, clipboard and composition.
pub const SEAT: SeatId = SeatId(0);

/// Fallback frame interval when the display reports no usable refresh rate.
const DEFAULT_HZ: f32 = 60.0;

/// An SDL window shared with its GPU leases.
///
/// The window is destroyed by whoever drops the last reference: removing the
/// window from the platform ends its input and presentation, and a lease taken
/// before that keeps the native window (and the handles pointing into it) alive
/// until the presenter is dropped.
pub(crate) struct NativeWindow {
    window: NonNull<sys::Window>,
    lib: &'static sys::Lib,
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: this is the last reference, and SDL is still initialized
        // because presenters are dropped before the platform they lease from.
        unsafe { (self.lib.destroy_window)(self.window.as_ptr()) };
    }
}

/// One SDL window and the platform state attached to it.
pub(crate) struct WindowState {
    pub(crate) id: WindowId,
    pub(crate) native: Rc<NativeWindow>,
    pub(crate) info: WindowInfo,
    /// The window was shown and not hidden again.
    pub(crate) shown: bool,
    /// SDL reports the window minimized; nothing is presented then.
    pub(crate) minimized: bool,
    /// The pointer is over this window, so a cursor change applies to it.
    pub(crate) mouse_focus: bool,
    pub(crate) dirty: bool,
    pub(crate) redraw_queued: bool,
    /// Earliest time the next frame of this window may be drawn.
    pub(crate) next_frame: Option<Instant>,
    /// Frame interval of the display the window is on.
    pub(crate) interval: Duration,
    pub(crate) cursor: Cursor,
    /// Software presentation budget, in bytes.
    pub(crate) budget: usize,
    /// Conversion scratch for window surfaces that are not tightly packed
    /// BGRA8, reused between frames.
    pub(crate) scratch: Vec<u8>,
    /// Damage rectangles of the frame being presented, reused.
    pub(crate) rects: Vec<sys::Rect>,
    /// Extent of the last presented frame, if any.
    pub(crate) drawn: Option<PixelSize>,
    /// The editor state last handed to the platform, `None` when no editable
    /// control is focused.
    pub(crate) ime: Option<ImeRequest>,
    /// Text input is enabled for this window (it has the keyboard focus).
    pub(crate) input_active: bool,
    /// Multiline property the active text input session was started with.
    pub(crate) multiline: Option<bool>,
    /// The host's answer for the drag over this window, `None` before one.
    pub(crate) drag_accept: Option<bool>,
    /// Files and text accumulated for the drag in progress.
    pub(crate) drag_files: Vec<std::path::PathBuf>,
    pub(crate) drag_text: Option<String>,
    /// Last known drag position, in logical window coordinates.
    pub(crate) drag_at: Option<Point>,
}

impl WindowState {
    pub(crate) fn ptr(&self) -> *mut sys::Window {
        self.native.window.as_ptr()
    }
}

/// A portal subscription followed on a helper thread.
struct Portal {
    /// A clone of the subscription socket, shut down to end the thread.
    socket: UnixStream,
    handle: Option<std::thread::JoinHandle<()>>,
    preferences: Arc<Mutex<Preferences>>,
    changed: Arc<AtomicBool>,
}

impl Drop for Portal {
    fn drop(&mut self) {
        // The reader is blocked on this socket; shutting it down makes the read
        // return end-of-file, so the thread finishes instead of outliving the
        // platform.
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// One SDL3 video connection, shared by all its windows.
///
/// This is a UI-thread owner: SDL requires video calls on the thread that
/// initialized it. Software frames are drawn only after a host change requests
/// a redraw, and at most once per display frame.
pub struct Sdl3 {
    pub(crate) lib: &'static sys::Lib,
    pub(crate) windows: Vec<WindowState>,
    pub(crate) events: VecDeque<Event>,
    next_id: u64,
    /// The user event `SDL_RegisterEvents` reserved for [`Self::wake_handle`].
    pub(crate) wake_type: u32,
    /// System cursors by [`cursor_index`], created on first use.
    cursors: [Option<NonNull<sys::Cursor>>; 10],
    /// Preferences last reported to the host.
    pub(crate) preferences: Preferences,
    portal: Option<Portal>,
    /// Whether the video driver can compose text through an input method.
    ime: bool,
    /// The application identifier last given to SDL, which keeps only one.
    app_id: Option<String>,
}

impl Sdl3 {
    /// Connects to the video driver SDL selects (Wayland or X11 on Linux, or
    /// the driver named by `SDL_VIDEODRIVER`). Fails with [`Error::Backend`]
    /// when `libSDL3` or a video driver is missing.
    pub(crate) fn connect() -> Result<Self, Error> {
        let lib = sys::lib().ok_or_else(|| {
            Error::Backend(
                "libSDL3.so.0 is not installed; install SDL3 or build with the wayland backend"
                    .into(),
            )
        })?;
        // SAFETY: the first SDL video call of the process on this thread, which
        // initializes SDL's video subsystem.
        if unsafe { !(lib.init)(sys::INIT_VIDEO) } {
            return Err(Error::backend(format!(
                "SDL_Init(SDL_INIT_VIDEO) failed: {}",
                sys::error(lib)
            )));
        }
        // SAFETY: SDL is initialized; a null driver name means the last attempt
        // failed, which INIT_VIDEO already reported.
        let driver = unsafe {
            let pointer = (lib.get_current_video_driver)();
            if pointer.is_null() {
                String::new()
            } else {
                CStr::from_ptr(pointer).to_string_lossy().into_owned()
            }
        };
        // One reserved user event code carries [WakeHandle] signals.
        // SAFETY: SDL is initialized; requesting one code has no preconditions.
        let wake_type = unsafe { (lib.register_events)(1) };
        if wake_type == u32::MAX {
            // SAFETY: undo the initialization above before reporting.
            unsafe { (lib.quit)() };
            return Err(Error::backend(format!(
                "SDL_RegisterEvents failed: {}",
                sys::error(lib)
            )));
        }
        let mut preferences = Preferences::default();
        let portal = start_portal(&mut preferences, wake_type, lib);
        // SDL reports the desktop's own theme directly, which is at least as
        // current as the portal's copy; the portal can be absent.
        let theme = theme_dark(lib);
        if theme.is_some() {
            preferences.dark = theme;
        }
        Ok(Self {
            lib,
            windows: Vec::new(),
            events: VecDeque::new(),
            next_id: 0,
            wake_type,
            cursors: [None; 10],
            preferences,
            portal,
            ime: !matches!(driver.as_str(), "" | "dummy" | "offscreen"),
            app_id: None,
        })
    }

    /// Appearance preferences known so far, from the desktop portal and SDL's
    /// system theme. Changes arrive as [`Event::Preferences`].
    pub(crate) fn preferences(&self) -> Preferences {
        self.preferences
    }

    /// One process-wide wake signal. SDL has a single event queue, so every
    /// handle shares the user event reserved at connection.
    pub(crate) fn wake_handle(&mut self) -> Result<WakeHandle, Error> {
        let lib = self.lib;
        let kind = self.wake_type;
        Ok(WakeHandle::new(move || {
            let event = sys::Event::user(kind);
            // SAFETY: SDL_PushEvent is thread-safe and copies the event; the
            // event holds no pointers and its code was reserved for this use.
            unsafe { (lib.push_event)(&event) };
        }))
    }

    /// Whether SDL's driver composes text through an input method. SDL3 does
    /// not expose the compositor's text-input availability, so a driver that
    /// supports composition reports it here even when the desktop has no input
    /// method installed.
    pub(crate) fn ime_available(&self) -> bool {
        self.ime
    }

    /// Creates an ordinary window. SDL3 has no desktop-shell layers, so a layer
    /// request fails with [`Error::Unsupported`] instead of becoming an
    /// ordinary window.
    pub(crate) fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId, Error> {
        if options.layer.is_some() {
            return Err(Error::Unsupported("layer surfaces"));
        }
        if options.size.width == 0 || options.size.height == 0 {
            return Err(Error::InvalidSize);
        }
        if [options.title, options.app_id]
            .iter()
            .any(|value| value.len() > 4000 || value.contains('\0'))
        {
            return Err(Error::InvalidString);
        }
        let title = sys::cstring(options.title).ok_or(Error::InvalidString)?;
        self.set_app_id(options.app_id);
        let lib = self.lib;
        let width = c_int::try_from(options.size.width).map_err(|_| Error::InvalidSize)?;
        let height = c_int::try_from(options.size.height).map_err(|_| Error::InvalidSize)?;
        // SAFETY: the title outlives the call and SDL copies it; the flags are
        // the documented window flags.
        let window = unsafe {
            (lib.create_window)(
                title.as_ptr(),
                width,
                height,
                sys::WINDOW_RESIZABLE | sys::WINDOW_HIDDEN | sys::WINDOW_HIGH_PIXEL_DENSITY,
            )
        };
        let window = NonNull::new(window).ok_or_else(|| {
            Error::backend(format!("SDL_CreateWindow failed: {}", sys::error(lib)))
        })?;
        let id = WindowId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Error::backend("window identity exhausted"))?;
        let state = WindowState {
            id,
            native: Rc::new(NativeWindow { window, lib }),
            info: WindowInfo {
                size: options.size,
                buffer: PixelSize::default(),
                scale: 1.0,
                active: false,
                configured: false,
            },
            shown: false,
            minimized: false,
            mouse_focus: false,
            dirty: true,
            redraw_queued: false,
            next_frame: None,
            interval: Duration::from_secs_f32(1.0 / DEFAULT_HZ),
            cursor: Cursor::Default,
            budget: options.buffer_budget,
            scratch: Vec::new(),
            rects: Vec::new(),
            drawn: None,
            ime: None,
            input_active: false,
            multiline: None,
            drag_accept: None,
            drag_files: Vec::new(),
            drag_text: None,
            drag_at: None,
        };
        self.windows.push(state);
        // A window reports its configure only once it is mapped.
        // SAFETY: the window is owned by this backend and shown once.
        unsafe { (lib.show_window)(window.as_ptr()) };
        if let Some(state) = self.windows.last_mut() {
            state.shown = true;
            state.interval = refresh_interval(lib, state.ptr());
        }
        self.reconfigure(id);
        Ok(id)
    }

    /// Removes a window, its queued events and any composition session.
    pub(crate) fn remove_window(&mut self, id: WindowId) -> Result<(), Error> {
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or(Error::InvalidWindow)?;
        // Dropping the last reference destroys the native window; a lease taken
        // before this call keeps it (and its handles) alive instead.
        self.windows.swap_remove(index);
        self.events.retain(|event| event.window() != Some(id));
        Ok(())
    }

    /// The latest configured geometry and activation.
    pub(crate) fn window_info(&self, id: WindowId) -> Result<WindowInfo, Error> {
        self.window(id).map(|window| window.info)
    }

    /// Marks a window's content changed; the redraw event follows at the next
    /// display frame.
    pub(crate) fn request_redraw(&mut self, id: WindowId) -> Result<(), Error> {
        self.window_mut(id)?.dirty = true;
        self.queue_redraws(Instant::now());
        Ok(())
    }

    /// Retitles a window.
    pub(crate) fn set_title(&mut self, id: WindowId, title: &str) -> Result<(), Error> {
        if title.len() > 4000 || title.contains('\0') {
            return Err(Error::InvalidString);
        }
        let title = sys::cstring(title).ok_or(Error::InvalidString)?;
        let window = self.window(id)?.ptr();
        // SAFETY: the window is live on this thread and SDL copies the title.
        if unsafe { !(self.lib.set_window_title)(window, title.as_ptr()) } {
            return Err(Error::backend(sys::error(self.lib)));
        }
        Ok(())
    }

    /// Sets the pointer shape over a window. SDL keeps one current cursor, so it
    /// applies at once only when the pointer is over this window; otherwise the
    /// shape is applied when the pointer enters it.
    pub(crate) fn set_cursor(&mut self, id: WindowId, cursor: Cursor) -> Result<(), Error> {
        let focused = {
            let window = self.window_mut(id)?;
            if window.cursor == cursor {
                return Ok(());
            }
            window.cursor = cursor;
            window.mouse_focus
        };
        if focused {
            self.apply_cursor(cursor);
        }
        Ok(())
    }

    pub(crate) fn apply_cursor(&mut self, cursor: Cursor) {
        let index = cursor_index(cursor);
        if self.cursors[index].is_none() {
            // SAFETY: creating a system cursor has no preconditions; the handle
            // is owned by this backend and destroyed in `drop`.
            let native = unsafe { (self.lib.create_system_cursor)(SYSTEM_CURSORS[index]) };
            self.cursors[index] = NonNull::new(native);
        }
        if let Some(native) = self.cursors[index] {
            // SAFETY: the cursor is live and owned here; SDL applies it to the
            // window with mouse focus on this thread.
            unsafe { (self.lib.set_cursor)(native.as_ptr()) };
        }
    }

    /// Waits at most `timeout` (`None`: indefinitely) for native work, or
    /// returns at once while events are queued or a frame is due. SDL's own
    /// wait is used, so there is no polling timer while the application idles.
    pub(crate) fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), Error> {
        let now = Instant::now();
        self.queue_redraws(now);
        let mut timeout = if self.events.is_empty() {
            timeout
        } else {
            Some(Duration::ZERO)
        };
        if let Some(deadline) = self.next_deadline(now) {
            timeout = Some(timeout.map_or(deadline, |timeout| timeout.min(deadline)));
        }
        let millis = match timeout {
            Some(timeout) if timeout.is_zero() => 0,
            Some(timeout) => timeout.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32,
            None => -1,
        };
        let mut event = sys::Event::zeroed();
        if millis != 0 {
            // SAFETY: SDL is initialized on this thread and the zeroed event is
            // written by SDL before it is read.
            if unsafe { (self.lib.wait_event_timeout)(&mut event, millis) } {
                self.translate(&event);
            }
        }
        // SAFETY: as above; polling copies one event at a time.
        while unsafe { (self.lib.poll_event)(&mut event) } {
            self.translate(&event);
        }
        self.queue_redraws(Instant::now());
        Ok(())
    }

    /// Pops the oldest queued event. Drain before blocking again.
    pub(crate) fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// A queued event pushed by this backend.
    pub(crate) fn emit(&mut self, event: Event) {
        self.events.push_back(event);
    }

    /// Queues a redraw for every window that changed and may present now.
    ///
    /// A window is served at most once per display frame. SDL3 has no surface
    /// frame callback, so the deadline from the display's refresh rate stands in
    /// for one; a later change waits for it in [`Self::dispatch`].
    pub(crate) fn queue_redraws(&mut self, now: Instant) {
        for window in &mut self.windows {
            if window.info.configured
                && window.dirty
                && !window.redraw_queued
                && window.next_frame.is_none_or(|at| now >= at)
            {
                window.redraw_queued = true;
                window.next_frame = Some(now + window.interval);
                let id = window.id;
                self.events.push_back(Event::Redraw { window: id });
            }
        }
    }

    /// How long until the earliest deferred frame, if any is waiting.
    fn next_deadline(&self, now: Instant) -> Option<Duration> {
        self.windows
            .iter()
            .filter(|window| window.info.configured && window.dirty && !window.redraw_queued)
            .filter_map(|window| window.next_frame)
            .map(|at| at.saturating_duration_since(now))
            .min()
    }

    /// Reads the window's current geometry, scale, activation and minimized
    /// state from SDL and reports a change as [`Event::Configure`].
    pub(crate) fn reconfigure(&mut self, id: WindowId) {
        let lib = self.lib;
        let Some(window) = self.windows.iter().find(|window| window.id == id) else {
            return;
        };
        // A hidden or zero-sized window keeps the extent it had; the configure
        // after it is mapped reports the real one.
        let Some((flags, size, buffer, display_scale)) = geometry(lib, window.ptr()) else {
            return;
        };
        // This ratio keeps the logical extent, the buffer and the damage
        // conversion consistent even when a display scale is only approximate.
        let scale = buffer.width as f32 / size.width as f32;
        let info = WindowInfo {
            size,
            buffer,
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                display_scale.max(1.0)
            },
            active: flags & sys::WINDOW_INPUT_FOCUS != 0,
            configured: window.shown && flags & sys::WINDOW_MINIMIZED == 0,
        };
        let minimized = flags & sys::WINDOW_MINIMIZED != 0;
        let Some(window) = self.windows.iter_mut().find(|window| window.id == id) else {
            return;
        };
        window.minimized = minimized;
        if window.info != info {
            window.info = info;
            window.dirty = true;
            window.redraw_queued = false;
            self.events.push_back(Event::Configure { window: id, info });
        }
    }

    /// Records a window's display refresh rate as its frame interval.
    pub(crate) fn update_interval(&mut self, id: WindowId) {
        let lib = self.lib;
        let Some(window) = self.windows.iter_mut().find(|window| window.id == id) else {
            return;
        };
        window.interval = refresh_interval(lib, window.ptr());
    }

    pub(crate) fn window(&self, id: WindowId) -> Result<&WindowState, Error> {
        self.windows
            .iter()
            .find(|window| window.id == id)
            .ok_or(Error::InvalidWindow)
    }

    pub(crate) fn window_mut(&mut self, id: WindowId) -> Result<&mut WindowState, Error> {
        self.windows
            .iter_mut()
            .find(|window| window.id == id)
            .ok_or(Error::InvalidWindow)
    }

    /// The contract identity of an SDL window ID, if this backend owns it.
    pub(crate) fn id_of(&self, native: u32) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|window| unsafe { (self.lib.get_window_id)(window.ptr()) } == native)
            .map(|window| window.id)
    }

    /// Redraws one window's changed region into its SDL surface and presents it.
    pub(crate) fn present(
        &mut self,
        id: WindowId,
        damage: Option<Region<PixelRect>>,
        draw: impl FnOnce(&mut [u8], PixelSize, &Region<PixelRect>) -> DrawResult,
    ) -> Result<bool, Error> {
        let lib = self.lib;
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or(Error::InvalidWindow)?;
        self.windows[index].redraw_queued = false;
        if !self.windows[index].info.configured {
            return Ok(false);
        }
        self.windows[index].dirty = false;
        let size = self.windows[index].info.buffer;
        if size.width == 0 || size.height == 0 {
            return Err(Error::InvalidSize);
        }
        let length = pixel_bytes(size)?;
        if length > self.windows[index].budget {
            return Err(Error::BufferBudget {
                required: length,
                budget: self.windows[index].budget,
            });
        }
        let native = self.windows[index].ptr();
        // SAFETY: a live window on this thread; the surface lives as long as the
        // window and is invalidated only by a resize, which is detected below.
        let surface = unsafe { (lib.get_window_surface)(native) };
        let Some(surface) = NonNull::new(surface) else {
            self.windows[index].dirty = true;
            return Err(Error::backend(format!(
                "SDL_GetWindowSurface failed: {}",
                sys::error(lib)
            )));
        };
        // SAFETY: SDL owns the surface until the window is resized or destroyed.
        let surface = unsafe { surface.as_ref() };
        let pixels = surface.pixels;
        let (width, height, pitch, format) = (surface.w, surface.h, surface.pitch, surface.format);
        let matches = width as u32 == size.width && height as u32 == size.height;
        if !matches || pixels.is_null() || pitch <= 0 {
            // SDL resized before the configure reached the host: report the new
            // extent and let the next redraw draw the whole window.
            self.windows[index].dirty = true;
            self.reconfigure(id);
            return Ok(false);
        }
        // The whole buffer is redrawn after a resize or a failed frame.
        let mut full = Region::default();
        full.add(PixelRect::full(size.width, size.height));
        let region = match self.windows[index].drawn {
            Some(drawn) if drawn == size => damage.unwrap_or(full),
            _ => full,
        };
        let stride = pitch as usize;
        let rows = size.height as usize;
        let tight = stride == size.width as usize * 4;
        let bgra = matches!(
            format,
            sys::PIXELFORMAT_XRGB8888 | sys::PIXELFORMAT_ARGB8888
        );
        if tight && bgra {
            // SAFETY: the surface owns `stride * rows` bytes at `pixels`, this
            // frame is the only borrow while the callback runs, and SDL does not
            // read the window surface until it is updated below.
            let pixels = unsafe { sys::bytes(pixels, stride * rows) };
            draw(pixels, size, &region).map_err(Error::Draw)?;
        } else {
            // SDL hands out padded rows or another pixel format: draw into a
            // tightly packed BGRA8 scratch buffer and let SDL convert it.
            let scratch = &mut self.windows[index].scratch;
            if scratch.len() != length {
                *scratch = Vec::new();
                scratch.try_reserve_exact(length).map_err(Error::backend)?;
                scratch.resize(length, 0);
            }
            draw(scratch, size, &region).map_err(Error::Draw)?;
            let source_pitch =
                c_int::try_from(size.width as usize * 4).map_err(|_| Error::InvalidSize)?;
            // SAFETY: both buffers hold at least one packed image of this
            // extent with the pitches given, and neither aliases the other.
            let converted = unsafe {
                (lib.convert_pixels)(
                    width,
                    height,
                    sys::PIXELFORMAT_ARGB8888,
                    scratch.as_ptr().cast(),
                    source_pitch,
                    format,
                    pixels,
                    pitch,
                )
            };
            if !converted {
                self.windows[index].dirty = true;
                return Err(Error::backend(format!(
                    "SDL_ConvertPixels failed: {}",
                    sys::error(lib)
                )));
            }
        }
        let rects = &mut self.windows[index].rects;
        rects.clear();
        rects.extend(
            region
                .rects()
                .iter()
                .filter(|rect| rect.width > 0 && rect.height > 0)
                .map(|rect| sys::Rect {
                    x: rect.x as c_int,
                    y: rect.y as c_int,
                    w: rect.width as c_int,
                    h: rect.height as c_int,
                }),
        );
        if !rects.is_empty() {
            // SAFETY: the window is live, the rectangles are inside the surface
            // extent and SDL reads them only for this call.
            let presented = unsafe {
                (lib.update_window_surface_rects)(native, rects.as_ptr(), rects.len() as c_int)
            };
            if !presented {
                self.windows[index].dirty = true;
                return Err(Error::backend(format!(
                    "SDL_UpdateWindowSurfaceRects failed: {}",
                    sys::error(lib)
                )));
            }
        }
        self.windows[index].drawn = Some(size);
        self.windows[index].dirty = false;
        Ok(true)
    }

    /// Presents through a GPU presenter. SDL keeps no frame completion signal,
    /// so pacing comes from the display deadline in [`Self::queue_redraws`] and
    /// from the swapchain's own synchronization.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub(crate) fn present_external(
        &mut self,
        id: WindowId,
        present: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool, Error> {
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or(Error::InvalidWindow)?;
        self.windows[index].redraw_queued = false;
        if !self.windows[index].info.configured {
            return Ok(false);
        }
        let size = self.windows[index].info.buffer;
        if size.width == 0 || size.height == 0 {
            return Err(Error::InvalidSize);
        }
        match present(size) {
            Ok(true) => {
                self.windows[index].dirty = false;
                Ok(true)
            }
            Ok(false) => {
                self.windows[index].dirty = true;
                Ok(false)
            }
            Err(error) => {
                self.windows[index].dirty = true;
                Err(Error::Draw(error))
            }
        }
    }

    /// Follows the portal's appearance settings after a wake signal.
    pub(crate) fn sync_portal(&mut self) {
        let Some(portal) = &self.portal else { return };
        if !portal.changed.swap(false, Ordering::AcqRel) {
            return;
        }
        let mut preferences = *portal.preferences.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(dark) = theme_dark(self.lib) {
            preferences.dark = Some(dark);
        }
        if preferences != self.preferences {
            self.preferences = preferences;
            self.events.push_back(Event::Preferences(preferences));
        }
    }

    /// Hands SDL the application identifier, which it keeps per process rather
    /// than per window.
    fn set_app_id(&mut self, app_id: &str) {
        if self.app_id.as_deref() == Some(app_id) {
            return;
        }
        let Some(native) = sys::cstring(app_id) else {
            return;
        };
        let name = c"SDL.app.metadata.identifier";
        // SAFETY: both strings are NUL-terminated and outlive the call, which
        // copies the value into SDL's process-wide metadata.
        unsafe { (self.lib.set_app_metadata_property)(name.as_ptr(), native.as_ptr()) };
        self.app_id = Some(app_id.to_owned());
    }

    /// Re-reads the system theme after SDL reported a change.
    pub(crate) fn theme_changed(&mut self) {
        let Some(dark) = theme_dark(self.lib) else {
            return;
        };
        if self.preferences.dark != Some(dark) {
            self.preferences.dark = Some(dark);
            self.events.push_back(Event::Preferences(self.preferences));
        }
    }
}

impl Drop for Sdl3 {
    fn drop(&mut self) {
        // The portal reader blocks on its socket; ending it first keeps the
        // subscription from outliving the connection it reports to.
        self.portal.take();
        for cursor in std::mem::take(&mut self.cursors).into_iter().flatten() {
            // SAFETY: this backend created the cursor and destroys it once,
            // before SDL is shut down.
            unsafe { (self.lib.destroy_cursor)(cursor.as_ptr()) };
        }
        // Dropping each window destroys it unless a lease still holds it, which
        // the contract forbids at this point.
        self.windows.clear();
        // SAFETY: the last SDL call of this backend on the initializing thread.
        unsafe { (self.lib.quit)() };
    }
}

/// A window's current geometry: flags, logical extent, physical extent and
/// display scale, or `None` while it reports no extent.
fn geometry(
    lib: &'static sys::Lib,
    window: *mut sys::Window,
) -> Option<(u64, PixelSize, PixelSize, f32)> {
    // SAFETY: the window is live on this thread; each call writes into the
    // out-parameters initialized here.
    unsafe {
        let mut width = 0;
        let mut height = 0;
        let mut pixel_width = 0;
        let mut pixel_height = 0;
        let flags = (lib.get_window_flags)(window);
        (lib.get_window_size)(window, &mut width, &mut height);
        (lib.get_window_size_in_pixels)(window, &mut pixel_width, &mut pixel_height);
        let size = PixelSize {
            width: u32::try_from(width).ok()?,
            height: u32::try_from(height).ok()?,
        };
        let buffer = PixelSize {
            width: u32::try_from(pixel_width).ok()?,
            height: u32::try_from(pixel_height).ok()?,
        };
        if size.width == 0 || size.height == 0 || buffer.width == 0 || buffer.height == 0 {
            return None;
        }
        Some((flags, size, buffer, (lib.get_window_display_scale)(window)))
    }
}

/// Bytes of a tightly packed BGRA8 buffer of `size`.
pub(crate) fn pixel_bytes(size: PixelSize) -> Result<usize, Error> {
    let pixels = u64::from(size.width)
        .checked_mul(u64::from(size.height))
        .ok_or(Error::InvalidSize)?;
    usize::try_from(pixels * 4).map_err(|_| Error::InvalidSize)
}

/// The display scale-independent frame interval of a window's display.
fn refresh_interval(lib: &'static sys::Lib, window: *mut sys::Window) -> Duration {
    // SAFETY: the window is live on this thread; the display mode is owned by
    // SDL and only read here.
    let hertz = unsafe {
        let display = (lib.get_display_for_window)(window);
        let mode = if display == 0 {
            std::ptr::null()
        } else {
            (lib.get_desktop_display_mode)(display)
        };
        if mode.is_null() {
            0.0
        } else {
            let mode = &*mode;
            if mode.refresh_rate_denominator > 0 && mode.refresh_rate_numerator > 0 {
                mode.refresh_rate_numerator as f32 / mode.refresh_rate_denominator as f32
            } else {
                mode.refresh_rate
            }
        }
    };
    let hertz = if hertz.is_finite() && (20.0..=1000.0).contains(&hertz) {
        hertz
    } else {
        DEFAULT_HZ
    };
    Duration::from_secs_f32(1.0 / hertz)
}

/// SDL's system theme as the contract's dark preference, `None` when SDL does
/// not know it.
fn theme_dark(lib: &'static sys::Lib) -> Option<bool> {
    // SAFETY: a query with no preconditions and no out-parameters.
    match unsafe { (lib.get_system_theme)() } {
        sys::THEME_DARK => Some(true),
        sys::THEME_UNKNOWN => None,
        _ => Some(false),
    }
}

/// Starts following the desktop portal's settings on a helper thread, because
/// SDL's wait cannot be combined with another socket. The thread blocks on the
/// portal's socket and pushes this backend's wake event when a value changes.
fn start_portal(
    preferences: &mut Preferences,
    wake_type: u32,
    lib: &'static sys::Lib,
) -> Option<Portal> {
    let (socket, mut portal) = aegle_portal::connect(preferences)?;
    // SAFETY: the socket is owned here and used from the reader thread only.
    socket.set_nonblocking(false).ok()?;
    let shared = Arc::new(Mutex::new(*preferences));
    let changed = Arc::new(AtomicBool::new(false));
    let handle = {
        let reader = socket.try_clone().ok()?;
        let shared = shared.clone();
        let changed = changed.clone();
        std::thread::Builder::new()
            .name("aegle-portal".into())
            .spawn(move || {
                let _reader = reader;
                let mut last = *shared.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    let mut next = last;
                    if !portal.read(&mut next) {
                        return;
                    }
                    if next != last {
                        last = next;
                        *shared.lock().unwrap_or_else(|e| e.into_inner()) = next;
                        changed.store(true, Ordering::Release);
                        let event = sys::Event::user(wake_type);
                        // SAFETY: SDL_PushEvent is thread-safe and copies the
                        // reserved user event.
                        unsafe { (lib.push_event)(&event) };
                    }
                }
            })
            .ok()?
    };
    Some(Portal {
        socket,
        handle: Some(handle),
        preferences: shared,
        changed,
    })
}

/// SDL cursor identifiers, in [`cursor_index`] order.
const SYSTEM_CURSORS: [c_int; 10] = [
    0,  // SDL_SYSTEM_CURSOR_DEFAULT
    1,  // SDL_SYSTEM_CURSOR_TEXT
    11, // SDL_SYSTEM_CURSOR_POINTER
    3,  // SDL_SYSTEM_CURSOR_CROSSHAIR
    9,  // SDL_SYSTEM_CURSOR_MOVE
    11, // SDL_SYSTEM_CURSOR_POINTER (no open hand in SDL)
    9,  // SDL_SYSTEM_CURSOR_MOVE (no closed hand in SDL)
    10, // SDL_SYSTEM_CURSOR_NOT_ALLOWED
    7,  // SDL_SYSTEM_CURSOR_EW_RESIZE
    8,  // SDL_SYSTEM_CURSOR_NS_RESIZE
];

/// The slot of `cursor` in [`SYSTEM_CURSORS`].
pub(crate) fn cursor_index(cursor: Cursor) -> usize {
    match cursor {
        Cursor::Default => 0,
        Cursor::Text => 1,
        Cursor::Pointer => 2,
        Cursor::Crosshair => 3,
        Cursor::Move => 4,
        Cursor::Grab => 5,
        Cursor::Grabbing => 6,
        Cursor::NotAllowed => 7,
        Cursor::ResizeHorizontal => 8,
        Cursor::ResizeVertical => 9,
    }
}

/// Whether the drag data SDL reported can be offered to the interface.
pub(crate) fn drag_data(
    files: &mut Vec<std::path::PathBuf>,
    text: &mut Option<String>,
) -> Option<DragData> {
    if !files.is_empty() {
        let files = std::mem::take(files);
        return Some(DragData::Files(files));
    }
    text.take().map(DragData::Text)
}
