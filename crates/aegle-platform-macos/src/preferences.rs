//! System appearance and accessibility preferences, and their change reports.
#![allow(unsafe_code)]
use crate::shared::Shared;
use aegle_platform::Preferences;
use block2::RcBlock;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSEvent, NSWorkspace,
    NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSObjectProtocol,
    NSOperationQueue,
};
use std::{ptr::NonNull, rc::Rc, time::Duration};

/// Reads the color scheme, increased contrast, reduced motion and double-click
/// interval. macOS has no system text-size setting, so that stays `None`.
pub(crate) fn read(mtm: MainThreadMarker) -> Preferences {
    let appearance = NSApplication::sharedApplication(mtm).effectiveAppearance();
    // SAFETY: static AppKit constants.
    let names = NSArray::from_slice(&unsafe { [NSAppearanceNameAqua, NSAppearanceNameDarkAqua] });
    let dark = appearance
        .bestMatchFromAppearancesWithNames(&names)
        .map(|name| &*name == unsafe { NSAppearanceNameDarkAqua });
    let workspace = NSWorkspace::sharedWorkspace();
    Preferences {
        dark,
        high_contrast: Some(workspace.accessibilityDisplayShouldIncreaseContrast()),
        reduced_motion: Some(workspace.accessibilityDisplayShouldReduceMotion()),
        text_scale: None,
        double_click: Some(Duration::from_secs_f64(NSEvent::doubleClickInterval())),
    }
}

/// A workspace observer that reports accessibility display changes; the
/// color scheme arrives through each view's appearance callback instead.
pub(crate) struct Observer {
    center: Retained<NSNotificationCenter>,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl Observer {
    pub fn new(mtm: MainThreadMarker, shared: Rc<Shared>) -> Self {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            shared.refresh_preferences(read(mtm));
        });
        // SAFETY: the block runs on the main queue, where it was made, and
        // the observer is removed before the platform goes away.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        };
        Self { center, token }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        // SAFETY: the token came from this center's block-based registration.
        // SAFETY: the token came from this center's block-based registration.
        let _: () = unsafe { objc2::msg_send![&self.center, removeObserver: &*self.token] };
    }
}
