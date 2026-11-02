//! Display enumeration and popup placement through SDL's display and popup
//! window calls.
#![allow(unsafe_code)]

use std::ffi::c_int;

use aegle_platform::{Error, OutputId, OutputInfo, PixelSize, Point, PopupOptions, Side};

use crate::{Sdl3, sys};

impl Sdl3 {
    /// The displays SDL knows, by SDL display ID. SDL names a display after
    /// what its video driver reports, which is not always a connector name;
    /// it reports no separate description.
    pub(crate) fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        let lib = self.lib;
        let mut count: c_int = 0;
        // SAFETY: SDL is initialized on this thread; the returned array holds
        // `count` IDs and is freed with SDL_free below.
        let ids = unsafe { (lib.get_displays)(&mut count) };
        if ids.is_null() {
            return Err(Error::backend(format!(
                "SDL_GetDisplays failed: {}",
                sys::error(lib)
            )));
        }
        // SAFETY: SDL wrote `count` IDs into the array it returned.
        let displays = unsafe { std::slice::from_raw_parts(ids, count.max(0) as usize) }.to_vec();
        // SAFETY: the array was allocated by SDL and is not used afterwards.
        unsafe { (lib.free)(ids.cast()) };
        Ok(displays
            .into_iter()
            .filter_map(|display| {
                let mut bounds = sys::Rect::default();
                // SAFETY: the display ID came from SDL_GetDisplays and the
                // rectangle is written before it is read.
                if unsafe { !(lib.get_display_bounds)(display, &mut bounds) } {
                    return None;
                }
                // SAFETY: SDL returns a NUL-terminated name owned by SDL, or null;
                // it is copied at once.
                let name = unsafe { sys::string((lib.get_display_name)(display)) };
                // SAFETY: a valid display ID; zero signals failure.
                let scale = unsafe { (lib.get_display_content_scale)(display) };
                Some(OutputInfo {
                    id: OutputId(display.into()),
                    name: name.unwrap_or_default(),
                    description: String::new(),
                    position: Point::new(bounds.x as f32, bounds.y as f32),
                    size: PixelSize {
                        width: bounds.w.max(0) as u32,
                        height: bounds.h.max(0) as u32,
                    },
                    scale: if scale > 0.0 { scale } else { 1.0 },
                })
            })
            .collect())
    }

    /// Creates a hidden SDL popup of `size` beside its anchor. SDL offers no
    /// anchor rectangle, so the popup is offset to the requested side and SDL
    /// slides it back onto the display; it does not flip sides.
    pub(crate) fn create_popup(
        &self,
        options: &PopupOptions,
        size: (c_int, c_int),
        flags: u64,
    ) -> Result<*mut sys::Window, Error> {
        let parent = self.window(options.parent)?.ptr();
        let rect = options.anchor;
        let finite = [
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        ]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= (i32::MAX / 4) as f32);
        if !finite || rect.size.width.round() < 1.0 || rect.size.height.round() < 1.0 {
            return Err(Error::InvalidRole("a popup anchor must be nonempty"));
        }
        let (x, y) = (
            rect.origin.x.round() as c_int,
            rect.origin.y.round() as c_int,
        );
        let (w, h) = (
            rect.size.width.round() as c_int,
            rect.size.height.round() as c_int,
        );
        let (x, y) = match options.side {
            Side::Below => (x, y + h),
            Side::Above => (x, y - size.1),
            Side::Left => (x - size.0, y),
            Side::Right => (x + w, y),
        };
        // A grabbing popup is a menu that takes the keyboard; SDL has one seat.
        let kind = match options.grab {
            Some(_) => sys::WINDOW_POPUP_MENU,
            None => sys::WINDOW_TOOLTIP,
        };
        let create = |flags| {
            // SAFETY: the parent is a live window of this backend; SDL copies
            // the geometry and returns a new window or null.
            unsafe { (self.lib.create_popup_window)(parent, x, y, size.0, size.1, flags | kind) }
        };
        let mut window = create(flags);
        if window.is_null() {
            window = create(flags & !sys::WINDOW_TRANSPARENT);
        }
        // Drivers without popups (dummy, offscreen) report SDL_Unsupported.
        if window.is_null() && sys::error(self.lib) == "That operation is not supported" {
            return Err(Error::Unsupported("popup surfaces"));
        }
        Ok(window)
    }
}
