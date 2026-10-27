//! The general pasteboard as plain text.
#![allow(unsafe_code)] // // Objective-C pasteboard calls need `unsafe`.

use crate::native::Native;
use aegle_platform::{Error, Event, Result};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

impl Native {
    /// Replaces the pasteboard's contents with `text`.
    pub(crate) fn set_clipboard(&self, text: &str) -> Result<()> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        // SAFETY: a static AppKit constant.
        let kind = unsafe { NSPasteboardTypeString };
        if pasteboard.setString_forType(&NSString::from_str(text), kind) {
            Ok(())
        } else {
            Err(Error::backend("the pasteboard refused the text"))
        }
    }

    /// Queues the pasteboard's text for the window; empty or non-text
    /// contents produce no event.
    pub(crate) fn request_clipboard(&self) {
        let pasteboard = NSPasteboard::generalPasteboard();
        // SAFETY: a static AppKit constant.
        let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString });
        if let Some(text) = text.map(|text| text.to_string()).filter(|t| !t.is_empty()) {
            self.emit(Event::Clipboard {
                window: self.id,
                seat: crate::SEAT,
                text,
            });
        }
    }
}
