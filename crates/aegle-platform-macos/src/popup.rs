//! Popups: borderless child windows placed beside an anchor of their parent.
//!
//! A popup is an `NSWindow` child of its parent, so AppKit moves, orders and
//! minimizes it with the parent. It is placed once, at creation, by
//! [`crate::popup_origin`] inside the visible area of the parent's screen.
//! A grabbing popup becomes the key window when shown, as a menu takes the
//! keyboard; AppKit has no popup grab, so the platform dismisses grabbing
//! popups itself: a press in a window outside a popup's chain, or the
//! application losing activation, reports [`Event::Close`] for it, innermost
//! first. The press is still delivered. A passive popup never becomes key.
#![allow(unsafe_code)] // Objective-C subclassing and initialization need `unsafe`.

use crate::{MacOs, native::Native, outputs, placement::popup_origin, shared::Shared};
use aegle_platform::{Error, Event, PopupOptions, Rect, Result, WindowId};
use aegle_types::Size;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{
    NSBackingStoreType, NSEvent, NSEventType, NSScreen, NSWindow, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use std::{cell::Cell, rc::Rc};

define_class!(
    // SAFETY: NSWindow has no subclassing requirements beyond those upheld by
    // the overrides below, and the class does not implement `Drop`.
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[ivars = bool]
    pub(crate) struct PopupWindow;

    impl PopupWindow {
        /// Borderless windows refuse key status by default; a grabbing popup
        /// takes the keyboard.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            *self.ivars()
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

/// A borderless popup window with `content` as its frame; `grab` lets it
/// become key.
pub(crate) fn window(mtm: MainThreadMarker, content: NSRect, grab: bool) -> Retained<NSWindow> {
    let this = mtm.alloc::<PopupWindow>().set_ivars(grab);
    // SAFETY: NSWindow's designated initializer, with the ivars set.
    let window: Retained<PopupWindow> = unsafe {
        msg_send![
            super(this),
            initWithContentRect: content,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // The interface draws its own shadows, as on Wayland.
    window.setHasShadow(false);
    Retained::into_super(window)
}

/// One popup's place in its chain, for dismissal.
pub(crate) struct Popup {
    pub id: WindowId,
    pub parent: WindowId,
    pub grab: bool,
    /// Whether its dismissal was reported, so it is reported once.
    pub dismissed: Cell<bool>,
}

impl Shared {
    /// Reports grabbing popups as dismissed, innermost first: every one when
    /// `pressed` is `None`, else those whose chain does not hold the pressed
    /// window, a popup's chain being itself and the popups opened from it.
    pub(crate) fn dismiss(&self, pressed: Option<WindowId>) {
        let popups = self.popups.borrow();
        let parent = |id: WindowId| {
            popups
                .iter()
                .find(|popup| popup.id == id)
                .map(|popup| popup.parent)
        };
        for popup in popups.iter().rev() {
            if !popup.grab || popup.dismissed.get() {
                continue;
            }
            let inside = std::iter::successors(pressed, |&id| parent(id)).any(|id| id == popup.id);
            if !inside {
                popup.dismissed.set(true);
                self.emit(Event::Close { window: popup.id });
            }
        }
    }
}

impl MacOs {
    /// Checks `options` and returns the parent to attach to.
    pub(crate) fn popup_parent(&self, options: &PopupOptions) -> Result<Rc<Native>> {
        let parent = self.window(options.parent)?.clone();
        let anchor = options.anchor;
        let finite = [
            anchor.origin.x,
            anchor.origin.y,
            anchor.size.width,
            anchor.size.height,
        ]
        .iter()
        .all(|value| value.is_finite());
        if !finite || anchor.size.width <= 0.0 || anchor.size.height <= 0.0 {
            return Err(Error::InvalidRole("a popup anchor must be nonempty"));
        }
        Ok(parent)
    }

    /// Reports the grabbing popups a mouse press dismisses, before AppKit
    /// delivers it.
    pub(crate) fn press_popups(&self, event: &NSEvent) {
        let press = matches!(
            event.r#type(),
            NSEventType::LeftMouseDown | NSEventType::RightMouseDown | NSEventType::OtherMouseDown
        );
        if !press || !self.shared.popups.borrow().iter().any(|popup| popup.grab) {
            return;
        }
        let pressed = event.window(self.mtm).and_then(|window| {
            self.windows
                .iter()
                .find(|native| Retained::as_ptr(&native.window) == Retained::as_ptr(&window))
                .map(|native| native.id)
        });
        self.shared.dismiss(pressed);
    }
}

impl Native {
    /// Moves the popup beside `options.anchor` of its parent, inside the
    /// visible area of the parent's screen.
    pub(crate) fn place(&self, mtm: MainThreadMarker, options: &PopupOptions) {
        let Some(parent) = &self.parent else {
            return;
        };
        let anchor = options.anchor;
        let height = parent.view.bounds().size.height;
        // The anchor in the parent view's y-up coordinates, then on screen.
        let local = NSRect::new(
            NSPoint::new(
                f64::from(anchor.origin.x),
                height - f64::from(anchor.origin.y + anchor.size.height),
            ),
            NSSize::new(f64::from(anchor.size.width), f64::from(anchor.size.height)),
        );
        let screen_rect = parent
            .window
            .convertRectToScreen(parent.view.convertRect_toView(local, None));
        let screen = parent.window.screen().or_else(|| NSScreen::mainScreen(mtm));
        let bounds = screen.map_or(Rect::new(-1e9, -1e9, 2e9, 2e9), |screen| {
            outputs::to_desktop(mtm, screen.visibleFrame())
        });
        let size = self.view.bounds().size;
        let origin = popup_origin(
            outputs::to_desktop(mtm, screen_rect),
            Size::new(size.width as f32, size.height as f32),
            options.side,
            bounds,
        );
        let (x, y) = outputs::from_desktop(mtm, origin, size.height);
        self.window.setFrameOrigin(NSPoint::new(x, y));
    }

    /// Attaches a popup to its parent, which orders it in, and gives a
    /// grabbing popup the keyboard.
    pub(crate) fn show_popup(&self, parent: &Native) {
        // SAFETY: both windows are live; the child is detached on removal.
        unsafe {
            parent
                .window
                .addChildWindow_ordered(&self.window, NSWindowOrderingMode::Above)
        };
        if self.grab {
            self.window.makeKeyAndOrderFront(None);
        } else {
            self.window.orderFront(None);
        }
    }
}
