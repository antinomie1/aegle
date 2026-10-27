//! The connection, its windows and their lifecycle.
#![allow(unsafe_code)]
use crate::{
    SEAT,
    app::AppDelegate,
    frame::Frame,
    ime::Composition,
    native::Native,
    preferences::{self, Observer},
    shared::Shared,
    view::View,
};
use aegle_platform::{
    Cursor, DrawResult, Error, Event, ImeEvent, PixelSize, Result, WakeHandle, WindowId,
    WindowInfo, WindowOptions,
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSCursor, NSPasteboardTypeFileURL, NSPasteboardTypeString,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
};
use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
    rc::Rc,
};

thread_local! { static CONNECTED: Cell<bool> = const { Cell::new(false) }; }

/// The most pixels along one window edge the backing store can hold.
const MAX_EDGE: u32 = 1 << 15;

/// The main-thread owner of the AppKit application, its windows and an
/// event-driven loop. Only one can exist per process.
pub struct MacOs {
    pub(crate) mtm: MainThreadMarker,
    pub(crate) app: Retained<NSApplication>,
    pub(crate) shared: Rc<Shared>,
    pub(crate) windows: Vec<Rc<Native>>,
    next_id: u64,
    /// The wake signal, created on first use, and the source it signals.
    pub(crate) wake: Option<WakeHandle>,
    pub(crate) waker: Option<crate::wake::Waker>,
    _delegate: Retained<AppDelegate>,
    _observer: Observer,
}

/// Owning main-thread lease on a window's native handles; keeps the view and
/// window alive through GPU and accessibility teardown. Removing the window
/// from the platform hides it and stops events at once.
#[derive(Clone)]
pub struct WindowSurface {
    native: Rc<Native>,
}

impl HasDisplayHandle for WindowSurface {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        // SAFETY: the AppKit display handle carries no data.
        Ok(unsafe {
            DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(AppKitDisplayHandle::new()))
        })
    }
}

impl HasWindowHandle for WindowSurface {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let view = NonNull::from(&*self.native.view).cast();
        // SAFETY: the lease holds the `Native` that retains this `NSView` for
        // the whole borrow, and the handle is only used on the main thread.
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(view)))
        })
    }
}

/// The system cursor closest to `cursor`. macOS has no four-arrow cursor, so
/// moving shows the closed hand.
pub(crate) fn cursor(cursor: Cursor) -> Retained<NSCursor> {
    match cursor {
        Cursor::Default => NSCursor::arrowCursor(),
        Cursor::Text => NSCursor::IBeamCursor(),
        Cursor::Pointer => NSCursor::pointingHandCursor(),
        Cursor::Crosshair => NSCursor::crosshairCursor(),
        Cursor::Grab => NSCursor::openHandCursor(),
        Cursor::Move | Cursor::Grabbing => NSCursor::closedHandCursor(),
        Cursor::NotAllowed => NSCursor::operationNotAllowedCursor(),
        #[allow(deprecated)]
        Cursor::ResizeHorizontal => NSCursor::resizeLeftRightCursor(),
        #[allow(deprecated)]
        Cursor::ResizeVertical => NSCursor::resizeUpDownCursor(),
    }
}

fn check_string(value: &str) -> Result<()> {
    if value.len() > 4000 || value.contains('\0') {
        Err(Error::InvalidString)
    } else {
        Ok(())
    }
}

impl MacOs {
    /// Starts the application on the main thread: a regular app with a
    /// minimal menu whose Quit asks every window to close.
    pub(crate) fn connect() -> Result<Self> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| Error::backend("AppKit must run on the main thread"))?;
        if CONNECTED.with(Cell::get) {
            return Err(Error::backend(
                "a macOS event loop already owns this process",
            ));
        }
        let shared = Rc::new(Shared::new(preferences::read(mtm)));
        let app = crate::app::start(mtm, shared.clone());
        CONNECTED.with(|value| value.set(true));
        Ok(Self {
            mtm,
            app: app.0,
            windows: Vec::new(),
            next_id: 0,
            wake: None,
            waker: None,
            _delegate: app.1,
            _observer: Observer::new(mtm, shared.clone()),
            shared,
        })
    }

    pub(crate) fn window(&self, id: WindowId) -> Result<&Rc<Native>> {
        self.windows
            .iter()
            .find(|native| native.id == id)
            .ok_or(Error::InvalidWindow)
    }

    /// Creates a hidden opaque titled window; the first successful frame
    /// shows it, so adapters and GPU surfaces can attach before that.
    pub(crate) fn create_window(&mut self, options: WindowOptions<'_>) -> Result<WindowId> {
        if options.layer.is_some() {
            return Err(Error::Unsupported("layer surfaces"));
        }
        let PixelSize { width, height } = options.size;
        if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
            return Err(Error::InvalidSize);
        }
        check_string(options.title)?;
        check_string(options.app_id)?;
        let id = WindowId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Error::backend("window identities exhausted"))?;
        let mtm = self.mtm;
        let content = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(f64::from(width), f64::from(height)),
        );
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // SAFETY: a standard titled window with a buffered backing store.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                content,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: the window is owned by `Native`, not by AppKit's close.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(options.title));
        window.setAcceptsMouseMovedEvents(true);
        let native = Rc::new_cyclic(|weak| Native {
            id,
            shared: self.shared.clone(),
            view: View::new(mtm, content, weak.clone()),
            window,
            info: Cell::new(WindowInfo {
                size: options.size,
                buffer: options.size,
                scale: 1.0,
                active: false,
                configured: true,
            }),
            registered: Cell::new(false),
            shown: Cell::new(false),
            occluded: Cell::new(false),
            dirty: Cell::new(true),
            redraw_queued: Cell::new(false),
            last_frame: Cell::new(None),
            cursor: Cell::new(Cursor::Default),
            inside: Cell::new(false),
            held: Cell::new(0),
            modifiers: Cell::new(Default::default()),
            ime: RefCell::new(Composition::new()),
            caret: Cell::new(aegle_platform::Rect::default()),
            frame: RefCell::new(Frame::default()),
            budget: options.buffer_budget,
            drag_accept: Cell::new(None),
            drag_active: Cell::new(false),
            drag_at: Cell::new(None),
            last_mouse: RefCell::new(None),
        });
        native.window.setContentView(Some(&native.view));
        native
            .window
            .setDelegate(Some(ProtocolObject::from_ref(&*native.view)));
        native.window.makeFirstResponder(Some(&native.view));
        // SAFETY: static AppKit constants.
        let kinds =
            NSArray::from_slice(&unsafe { [NSPasteboardTypeFileURL, NSPasteboardTypeString] });
        native.view.registerForDraggedTypes(&kinds);
        native.window.center();
        // Report the real scale and size, which depend on the screen.
        let info = native.measure();
        native.info.set(info);
        native.registered.set(true);
        native.emit(Event::Configure { window: id, info });
        native.queue_redraw(std::time::Instant::now());
        self.shared.windows.borrow_mut().push(id);
        self.windows.push(native);
        Ok(id)
    }

    /// Stops events and hides the window; its view and window are released
    /// with the last surface lease.
    pub(crate) fn remove_window(&mut self, id: WindowId) -> Result<()> {
        let index = self
            .windows
            .iter()
            .position(|native| native.id == id)
            .ok_or(Error::InvalidWindow)?;
        let native = self.windows.remove(index);
        native.registered.set(false);
        native.cancel_ime();
        native.window.setDelegate(None);
        native.window.orderOut(None);
        self.shared
            .windows
            .borrow_mut()
            .retain(|window| *window != id);
        self.shared
            .events
            .borrow_mut()
            .retain(|event| event.window() != Some(id));
        Ok(())
    }

    pub(crate) fn window_surface(&self, id: WindowId) -> Result<WindowSurface> {
        Ok(WindowSurface {
            native: self.window(id)?.clone(),
        })
    }

    pub(crate) fn set_title(&mut self, id: WindowId, title: &str) -> Result<()> {
        check_string(title)?;
        self.window(id)?.window.setTitle(&NSString::from_str(title));
        Ok(())
    }

    /// Sets the pointer shape over the content, at once if the pointer is
    /// inside and on every later entry.
    pub(crate) fn set_cursor(&mut self, id: WindowId, shape: Cursor) -> Result<()> {
        let native = self.window(id)?;
        native.cursor.set(shape);
        native.window.invalidateCursorRectsForView(&native.view);
        if native.inside.get() {
            cursor(shape).set();
        }
        Ok(())
    }

    /// Executes one native GPU submission when the window is ready, then
    /// shows it. The GPU presenter owns drawable acquisition and pacing.
    pub(crate) fn present_external(
        &mut self,
        id: WindowId,
        draw: impl FnOnce(PixelSize) -> DrawResult<bool>,
    ) -> Result<bool> {
        let native = self.window(id)?.clone();
        native.redraw_queued.set(false);
        let info = native.info.get();
        if !info.configured {
            return Ok(false);
        }
        // A window must be on screen before the compositor gives it drawables.
        native.show(self.mtm);
        if !draw(info.buffer).map_err(Error::Draw)? {
            return Ok(false);
        }
        native.dirty.set(false);
        native.last_frame.set(Some(std::time::Instant::now()));
        Ok(true)
    }
}

impl Native {
    /// Reports the window gaining or losing keyboard focus, with the input
    /// method's session as Win32 does: it ends with focus and the host
    /// re-enables it when focus returns.
    pub(crate) fn key_changed(&self, focused: bool) {
        self.refresh();
        self.emit(Event::KeyboardFocus {
            window: self.id,
            seat: SEAT,
            focused,
        });
        if focused {
            if self.ime_enabled() {
                self.emit_ime(ImeEvent::Entered);
            }
        } else {
            self.cancel_ime();
            self.emit_ime(ImeEvent::Left);
        }
    }
}

impl Drop for MacOs {
    fn drop(&mut self) {
        while let Some(id) = self.windows.last().map(|native| native.id) {
            let _ = self.remove_window(id);
        }
        CONNECTED.with(|value| value.set(false));
    }
}
