//! Ordinary windows with or without the system's title bar.
//!
//! An undecorated window drops only the `Titled` style, so it still zooms,
//! miniaturizes and enters full screen. AppKit resizes only titled windows
//! by their edges; `area` gives an undecorated one its resize border. A
//! window without a title bar refuses key and main status unless its class
//! allows them.
#![allow(unsafe_code)] // Objective-C subclassing and initialization need `unsafe`.

use crate::{MacOs, native::Native};
use aegle_platform::{Result, WindowId};
use objc2::{MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{MainThreadMarker, NSRect};

define_class!(
    // SAFETY: NSWindow has no subclassing requirements beyond those upheld by
    // the overrides below, and the class does not implement `Drop`.
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    struct Toplevel;

    impl Toplevel {
        /// Windows without a title bar refuse key status by default.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }
    }
);

/// A decorated, resizable ordinary window around `content`.
pub(crate) fn window(mtm: MainThreadMarker, content: NSRect) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let this = mtm.alloc::<Toplevel>().set_ivars(());
    // SAFETY: NSWindow's designated initializer for a buffered window.
    let window: Retained<Toplevel> = unsafe {
        msg_send![
            super(this),
            initWithContentRect: content,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    Retained::into_super(window)
}

impl MacOs {
    pub(crate) fn set_decorations(&mut self, id: WindowId, decorated: bool) -> Result<()> {
        let native = self.ordinary(id)?;
        native.decorated.set(decorated);
        native.apply_decorations();
        Ok(())
    }
}

impl Native {
    /// Gives the window the title bar last requested, keeping its content
    /// where it is. A full-screen window waits until it has left full
    /// screen, whose style AppKit owns meanwhile.
    pub(crate) fn apply_decorations(&self) {
        let mut style = self.window.styleMask();
        let decorated = self.decorated.get();
        if style.contains(NSWindowStyleMask::Titled) == decorated
            || style.contains(NSWindowStyleMask::FullScreen)
            || self.transitioning.get()
        {
            return;
        }
        let content = self.window.contentRectForFrameRect(self.window.frame());
        style.set(NSWindowStyleMask::Titled, decorated);
        self.restyle(style);
        self.window
            .setFrame_display(self.window.frameRectForContentRect(content), true);
        self.refresh();
    }
}
