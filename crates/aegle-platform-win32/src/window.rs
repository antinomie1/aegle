#![allow(unsafe_code)]
use crate::{
    Error, Event, ImeRequest, PixelSize, PresentError, WindowId, WindowInfo, WindowOptions,
    ime::Ime,
    native::{Native, Queue},
    procedure::procedure,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    Win32WindowHandle, WindowHandle,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    num::NonZeroIsize,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    rc::Rc,
    sync::{Arc, OnceLock},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{HANDLE, HINSTANCE, HWND, RECT, WAIT_FAILED, WAIT_OBJECT_0},
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{CreateEventW, SetEvent},
        },
        UI::{
            HiDpi::*,
            Input::Ime::{HIMC, ImmAssociateContext},
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

thread_local! { static CONNECTED: Cell<bool> = const { Cell::new(false) }; }
static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();

/// UI-thread owner of native windows and an event-driven Windows message loop.
/// Only one Win32 backend can own a given UI thread at a time.
pub struct Win32 {
    windows: Vec<Rc<Native>>,
    events: Queue,
    preferences: Rc<Cell<crate::Preferences>>,
    next_id: u64,
    wake: Option<WakeHandle>,
    previous_dpi: DPI_AWARENESS_CONTEXT,
}

/// Owning UI-thread surface lease; keeps HWND alive through GPU/UIA destruction.
/// Removing its window from the backend hides it and stops events immediately.
#[derive(Clone)]
pub struct WindowSurface {
    native: Rc<Native>,
}
impl HasDisplayHandle for WindowSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}
impl HasWindowHandle for WindowSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd =
            NonZeroIsize::new(self.native.hwnd.get().0 as isize).ok_or(HandleError::Unavailable)?;
        let mut handle = Win32WindowHandle::new(hwnd);
        // SAFETY: our HWND was created by this module, which remains loaded.
        handle.hinstance = NonZeroIsize::new(
            unsafe { GetModuleHandleW(None) }
                .map_err(|_| HandleError::Unavailable)?
                .0 as isize,
        );
        // SAFETY: this lease holds the stable Native owner for the entire borrow.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

/// Cloneable cross-thread signal; retains only an auto-reset OS event.
#[derive(Clone, Debug)]
pub struct WakeHandle(Arc<OwnedHandle>);
impl WakeHandle {
    /// Signals the UI loop after work is stored in the caller's own queue.
    /// Requests coalesce; a handle cannot resurrect a removed event loop.
    pub fn wake(&self) {
        // SAFETY: Arc retains the owned event HANDLE until this call returns.
        unsafe {
            let _ = SetEvent(HANDLE(self.0.as_raw_handle()));
        }
    }
}

impl Win32 {
    /// Registers one small native class and enters per-monitor-v2 DPI awareness.
    /// Restores the previous calling-thread DPI context when dropped.
    pub fn connect() -> Result<Self, Error> {
        if CONNECTED.with(Cell::get) {
            return Err(Error::Backend(
                "a Win32 event loop already owns this UI thread".into(),
            ));
        }
        let class = CLASS.get_or_init(|| {
            // SAFETY: immutable class data; WNDPROC's lifetime is the loaded module.
            unsafe {
                let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                let class = WNDCLASSW {
                    style: CS_DBLCLKS,
                    lpfnWndProc: Some(procedure),
                    hInstance: HINSTANCE(module.0),
                    hCursor: LoadCursorW(None, IDC_ARROW).map_err(|e| e.to_string())?,
                    lpszClassName: w!("Aegle.NativeWindow.v1"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&class);
                if atom == 0 {
                    Err(windows::core::Error::from_thread().to_string())
                } else {
                    Ok(atom)
                }
            }
        });
        class.as_ref().map_err(|e| Error::Backend(e.clone()))?;
        // SAFETY: changes only this calling thread, original context is restored.
        let previous_dpi =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous_dpi.0.is_null() {
            return Err(windows::core::Error::from_thread().into());
        }
        CONNECTED.with(|value| value.set(true));
        Ok(Self {
            windows: Vec::new(),
            events: Rc::new(RefCell::new(VecDeque::new())),
            preferences: Rc::new(Cell::new(crate::preferences::read())),
            next_id: 0,
            wake: None,
            previous_dpi,
        })
    }

    /// Lazily creates an auto-reset event for accessibility/background work.
    pub fn wake_handle(&mut self) -> Result<WakeHandle, Error> {
        if self.wake.is_none() {
            // SAFETY: no name or borrowed security attributes; unique owned handle.
            let event = unsafe { CreateEventW(None, false, false, None)? };
            let owned = unsafe { OwnedHandle::from_raw_handle(event.0) };
            self.wake = Some(WakeHandle(Arc::new(owned)));
        }
        Ok(self.wake.as_ref().unwrap().clone())
    }

    /// Creates a hidden opaque toplevel; the first successful frame shows it.
    /// UIA adapters and Vulkan surfaces can attach before that first presentation.
    pub fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId, Error> {
        options.size.bytes()?;
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
            .ok_or_else(|| Error::Backend("window identities exhausted".into()))?;
        // SAFETY: process system DPI, then actual monitor DPI is read after creation.
        let dpi = unsafe { GetDpiForSystem() };
        let scale = dpi as f32 / 96.0;
        let width = (options.size.width as f64 * f64::from(scale)).ceil();
        let height = (options.size.height as f64 * f64::from(scale)).ceil();
        if width > i32::MAX as f64 / 4.0 || height > i32::MAX as f64 {
            return Err(Error::InvalidSize);
        }
        let physical = PixelSize {
            width: width as u32,
            height: height as u32,
        };
        let native = Rc::new(Native {
            hwnd: Cell::new(HWND::default()),
            id,
            events: self.events.clone(),
            preferences: self.preferences.clone(),
            info: Cell::new(WindowInfo {
                size: options.size,
                physical,
                scale,
                active: false,
                configured: true,
            }),
            registered: Cell::new(false),
            dirty: Cell::new(true),
            redraw_queued: Cell::new(false),
            tracking: Cell::new(false),
            cursor: Cell::new(aegle_types::Cursor::Default),
            held: Cell::new(0),
            high_surrogate: Cell::new(None),
            ime: Ime::new()?,
            pixels: RefCell::new(Vec::new()),
            drawn: Cell::new(None),
            budget: options.buffer_budget,
        });
        let title: Vec<u16> = options.title.encode_utf16().chain(Some(0)).collect();
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: physical.width as i32,
            bottom: physical.height as i32,
        };
        // SAFETY: class is registered, strings terminate in NUL, stable Native Rc
        // remains alive through creation callbacks and all later native leases.
        unsafe {
            AdjustWindowRectExForDpi(
                &mut rect,
                WS_OVERLAPPEDWINDOW,
                false,
                WINDOW_EX_STYLE(0),
                dpi,
            )?;
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("Aegle.NativeWindow.v1"),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
                None,
                None,
                Some(HINSTANCE(GetModuleHandleW(None)?.0)),
                Some(Rc::as_ptr(&native).cast()),
            )?;
            // Each window gets its own context on demand; the shared default IMM
            // context must not cause a composition to leak into another editor.
            ImmAssociateContext(hwnd, HIMC::default());
            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client)?;
            native.configure(
                PixelSize {
                    width: client.right as u32,
                    height: client.bottom as u32,
                },
                GetDpiForWindow(hwnd) as f32 / 96.0,
                false,
                true,
            );
        }
        native.registered.set(true);
        native.emit(Event::Configure {
            window: id,
            info: native.info.get(),
        });
        native.queue_redraw();
        self.windows.push(native);
        Ok(id)
    }

    /// Stops events and hides the window. Underlying HWND destruction waits until
    /// the last GPU/UIA surface lease is dropped on this thread.
    pub fn remove_window(&mut self, id: WindowId) -> Result<(), Error> {
        let index = self
            .windows
            .iter()
            .position(|w| w.id == id)
            .ok_or(Error::InvalidWindow)?;
        let native = self.windows.swap_remove(index);
        native.registered.set(false);
        native.cancel_ime();
        // SAFETY: live owned HWND; hiding it does not invalidate surface leases.
        unsafe {
            let _ = ShowWindow(native.hwnd.get(), SW_HIDE);
        }
        self.events.borrow_mut().retain(|e| e.target() != Some(id));
        Ok(())
    }

    /// Obtains an owned surface lease suitable for raw-window-handle consumers.
    pub fn window_surface(&self, id: WindowId) -> Result<WindowSurface, Error> {
        Ok(WindowSurface {
            native: self.window(id)?.clone(),
        })
    }
    /// Returns current exact physical and rounded logical client geometry.
    pub fn window_info(&self, id: WindowId) -> Result<WindowInfo, Error> {
        Ok(self.window(id)?.info.get())
    }
    /// Returns retained software Vec capacity, excluding GDI/compositor storage.
    pub fn buffer_bytes(&self, id: WindowId) -> Result<usize, Error> {
        Ok(self.window(id)?.pixels.borrow().capacity())
    }
    /// Sets the shape shown over a window's client area. It takes effect at once
    /// while the mouse is over the window, and on every later WM_SETCURSOR.
    pub fn set_cursor(&mut self, id: WindowId, cursor: aegle_types::Cursor) -> Result<(), Error> {
        let native = self.window(id)?;
        native.cursor.set(cursor);
        if native.tracking.get() {
            native.show_cursor();
        }
        Ok(())
    }
    /// Marks retained content dirty; requests coalesce and minimized windows wait.
    pub fn request_redraw(&mut self, id: WindowId) -> Result<(), Error> {
        let native = self.window(id)?;
        native.dirty.set(true);
        native.queue_redraw();
        Ok(())
    }
    /// Native IMM compatibility is available. This does not imply TSF text-store,
    /// surrounding-text reconversion or an installed language/input method.
    pub fn ime_available(&self) -> bool {
        true
    }
    /// Enables or cancels the current editable control's native IMM session.
    /// Disable before changing editor identity, even within the same window.
    pub fn configure_ime(
        &mut self,
        id: WindowId,
        request: Option<ImeRequest>,
    ) -> Result<(), Error> {
        crate::ime::configure(self.window(id)?, request)
    }
    /// Current system appearance preferences. Changes arrive as
    /// [`Event::Preferences`] while a window exists to receive the broadcast.
    pub fn preferences(&self) -> crate::Preferences {
        self.preferences.get()
    }
    /// Pops one queued event. Drain these before waiting for more native input.
    pub fn next_event(&mut self) -> Option<Event> {
        self.events.borrow_mut().pop_front()
    }

    /// Waits without polling until native input, a wake signal or the deadline.
    /// Returns after messages produce host events so the host can cancel an old
    /// IME session before later native messages are translated.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), Error> {
        for native in &self.windows {
            native.queue_redraw();
        }
        // Ready frames must not starve native input during continuous animation.
        let timeout = if self.events.borrow().is_empty() {
            timeout
        } else {
            Some(Duration::ZERO)
        };
        let milliseconds = timeout.map_or(u32::MAX, |t| {
            t.as_millis()
                .saturating_add(u128::from(t.subsec_nanos() % 1_000_000 != 0))
                .min(u128::from(u32::MAX - 1)) as u32
        });
        let handle = self
            .wake
            .as_ref()
            .map(|wake| [HANDLE(wake.0.as_raw_handle())]);
        // SAFETY: optional handle is kept alive by self; waiting dispatches no callbacks.
        let status = unsafe {
            MsgWaitForMultipleObjectsEx(
                handle.as_ref().map(|h| h.as_slice()),
                milliseconds,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        if status == WAIT_FAILED {
            return Err(windows::core::Error::from_thread().into());
        }
        if handle.is_some() && status == WAIT_OBJECT_0 {
            self.events.borrow_mut().push_back(Event::Wake);
        }
        let mut message = MSG::default();
        // SAFETY: initialized writable MSG, UI thread translates/dispatches it.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            let queued = self.events.borrow().len();
            if message.message == WM_QUIT {
                for window in &self.windows {
                    window.emit(Event::Close { window: window.id });
                }
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if self.events.borrow().len() != queued {
                break;
            }
        }
        Ok(())
    }

    /// Executes one native GPU submission/presentation when the window is ready.
    /// This does not copy pixels or change presentation synchronization; the
    /// Vulkan presenter owns FIFO pacing, acquire/present and resize handling.
    pub fn present_external<E>(
        &mut self,
        id: WindowId,
        draw: impl FnOnce(PixelSize) -> Result<bool, E>,
    ) -> Result<bool, PresentError<E>> {
        let native = self.window(id).map_err(PresentError::Platform)?;
        native.redraw_queued.set(false);
        if !native.info.get().configured {
            return Ok(false);
        }
        let size = native
            .info
            .get()
            .buffer_size()
            .map_err(PresentError::Platform)?;
        if !draw(size).map_err(PresentError::Draw)? {
            return Ok(false);
        }
        native.dirty.set(false);
        // SAFETY: owned HWND; first successful frame is now available for display.
        unsafe {
            if !IsWindowVisible(native.hwnd.get()).as_bool() {
                let _ = ShowWindow(native.hwnd.get(), SW_SHOWNORMAL);
            }
        }
        Ok(true)
    }

    pub(crate) fn window(&self, id: WindowId) -> Result<&Rc<Native>, Error> {
        self.windows
            .iter()
            .find(|w| w.id == id)
            .ok_or(Error::InvalidWindow)
    }
}
impl Drop for Win32 {
    fn drop(&mut self) {
        while let Some(window) = self.windows.last() {
            let _ = self.remove_window(window.id);
        }
        // SAFETY: restore exactly the calling thread's previous DPI context.
        unsafe {
            SetThreadDpiAwarenessContext(self.previous_dpi);
        }
        CONNECTED.with(|value| value.set(false));
    }
}
