//! The native text services of a window: composition, the clipboard and drags.
//!
//! SDL3 exposes one clipboard and one text-input session per window, so the
//! contract's per-seat arguments all name the single seat of an SDL
//! application. Composition is SDL's own model: an editing event replaces the
//! preedit, a text-input event commits it, and the surrounding text and
//! selection of [`ImeRequest`] cannot be reported to the platform. What the
//! request does carry is used: the caret's area places the candidate window and
//! whether the editor is multiline becomes the input type.
#![allow(unsafe_code)]

use aegle_platform::{
    DragData, Error, Event, ImeEvent, ImeRequest, Rect, Result, SeatId, WindowId,
};

use crate::{sys, window::Sdl3};

impl Sdl3 {
    /// Enables, updates or disables composition for the focused editor.
    ///
    /// SDL3 has no surrounding-text or selection protocol, so those fields are
    /// validated and then dropped; the caret rectangle and the multiline flag
    /// are applied. A request fails with [`Error::ImeUnavailable`] on a driver
    /// without composition.
    pub(crate) fn configure_ime(
        &mut self,
        id: WindowId,
        request: Option<ImeRequest>,
    ) -> Result<()> {
        self.window_info(id)?;
        if !self.ime_available() && request.is_some() {
            return Err(Error::ImeUnavailable);
        }
        let Some(request) = request else {
            if let Ok(window) = self.window_mut(id) {
                window.ime = None;
            }
            // Updates already queued belong to the editor that lost focus.
            self.events.retain(|event| {
                !matches!(event,
                Event::Ime { window, event: ImeEvent::Update(_), .. } if *window == id)
            });
            return Ok(());
        };
        request.validate()?;
        let area = input_area(&request)?;
        let (active, multiline, native) = {
            let window = self.window(id)?;
            (window.input_active, window.multiline, window.ptr())
        };
        if active {
            // SAFETY: the window is live on this thread; SDL copies the area.
            unsafe { (self.lib.set_text_input_area)(native, &area, 0) };
            if multiline != Some(request.multiline) {
                self.restart_input(id, request.multiline);
            }
        }
        if let Ok(window) = self.window_mut(id) {
            window.ime = Some(request);
        }
        Ok(())
    }

    /// Starts text input for a window that gained the keyboard focus. SDL sends
    /// no text events for a window without it.
    pub(crate) fn start_input(&mut self, id: WindowId) {
        let Ok(window) = self.window(id) else { return };
        let multiline = window.multiline.unwrap_or(false);
        self.restart_input(id, multiline);
    }

    /// Stops text input for a window that lost the keyboard focus; a
    /// composition cannot continue without it.
    pub(crate) fn stop_input(&mut self, id: WindowId) {
        let Ok(window) = self.window(id) else { return };
        let native = window.ptr();
        // SAFETY: the window is live on this thread.
        unsafe { (self.lib.stop_text_input)(native) };
        let mut left = false;
        if let Ok(window) = self.window_mut(id) {
            window.input_active = false;
            left = window.ime.is_some();
        }
        if left {
            self.emit(Event::Ime {
                window: id,
                seat: crate::window::SEAT,
                event: ImeEvent::Left,
            });
        }
    }

    /// (Re)starts text input with the input type an editor asked for. SDL pairs
    /// every start with a stop, and the multiline property is fixed for a
    /// session, so a changed editor restarts it.
    fn restart_input(&mut self, id: WindowId, multiline: bool) {
        let lib = self.lib;
        let Ok(window) = self.window(id) else { return };
        let native = window.ptr();
        // SAFETY: the window is live on this thread.
        unsafe { (lib.stop_text_input)(native) };
        // SAFETY: the properties are created, filled and destroyed here; SDL
        // copies what it needs while starting the session.
        let started = unsafe {
            let properties = (lib.create_properties)();
            if properties == 0 {
                (lib.start_text_input)(native)
            } else {
                let key = c"SDL.textinput.multiline";
                (lib.set_boolean_property)(properties, key.as_ptr(), multiline);
                let started = (lib.start_text_input_with_properties)(native, properties);
                (lib.destroy_properties)(properties);
                started
            }
        };
        if let Ok(window) = self.window_mut(id) {
            window.input_active = started;
            window.multiline = Some(multiline);
        }
    }

    /// Replaces the clipboard with `text`; SDL has one clipboard for the
    /// process, so every seat shares it.
    pub(crate) fn set_clipboard(&mut self, id: WindowId, _seat: SeatId, text: &str) -> Result<()> {
        self.window_info(id)?;
        let text = sys::cstring(text)
            .ok_or_else(|| Error::backend("clipboard text contains a NUL byte"))?;
        // SAFETY: the string is NUL-terminated and outlives the call, which
        // copies it into SDL's clipboard.
        if unsafe { !(self.lib.set_clipboard_text)(text.as_ptr()) } {
            return Err(Error::backend(format!(
                "SDL_SetClipboardText failed: {}",
                sys::error(self.lib)
            )));
        }
        Ok(())
    }

    /// Queues the clipboard's text as [`Event::Clipboard`]; nothing arrives
    /// when it holds no text.
    pub(crate) fn request_clipboard(&mut self, id: WindowId, seat: SeatId) -> Result<()> {
        self.window_info(id)?;
        let lib = self.lib;
        // SAFETY: SDL returns a new allocation the caller frees, or null.
        let pointer = unsafe { (lib.get_clipboard_text)() };
        if pointer.is_null() {
            return Err(Error::backend(format!(
                "SDL_GetClipboardText failed: {}",
                sys::error(lib)
            )));
        }
        // SAFETY: the allocation is a NUL-terminated UTF-8 string owned by this
        // call, and it is freed after the copy.
        let text = unsafe { sys::string(pointer) };
        // SAFETY: the pointer came from SDL and is freed exactly once.
        unsafe { (lib.free)(pointer.cast()) };
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.emit(Event::Clipboard {
                window: id,
                seat,
                text,
            });
        }
        Ok(())
    }

    /// Records whether the interface accepts the drag over this window. SDL
    /// delivers drops unconditionally, so a refusal is honoured by answering
    /// [`Event::DragLeave`] instead of [`Event::Drop`] at the end of the drag.
    pub(crate) fn accept_drag(&mut self, id: WindowId, _seat: SeatId, accept: bool) -> Result<()> {
        self.window_info(id)?;
        if let Ok(window) = self.window_mut(id) {
            window.drag_accept = Some(accept);
        }
        Ok(())
    }

    /// SDL3 cannot begin a drag from an application window: it only receives
    /// drags from other clients.
    pub(crate) fn start_drag(
        &mut self,
        id: WindowId,
        _seat: SeatId,
        _data: &DragData,
    ) -> Result<()> {
        self.window_info(id)?;
        Err(Error::Unsupported("starting a drag from an SDL3 window"))
    }
}

/// The caret or selection area in the native input coordinates, rounded
/// outward so a fractionally placed caret stays inside it.
fn input_area(request: &ImeRequest) -> Result<sys::Rect> {
    let Rect { origin, size } = request.cursor_rect;
    let left = origin.x.floor();
    let top = origin.y.floor();
    let right = (origin.x + size.width).ceil();
    let bottom = (origin.y + size.height).ceil();
    let fits = |value: f32| (i32::MIN as f32..=i32::MAX as f32).contains(&value);
    if ![left, top, right, bottom].iter().all(|value| fits(*value)) {
        return Err(Error::InvalidIme(
            "cursor rectangle does not fit native coordinates",
        ));
    }
    Ok(sys::Rect {
        x: left as i32,
        y: top as i32,
        w: (right - left) as i32,
        h: (bottom - top) as i32,
    })
}
