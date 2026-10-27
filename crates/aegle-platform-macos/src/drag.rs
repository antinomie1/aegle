//! Drag and drop: the view is a destination for text and files, and
//! [`Native::start_drag`] begins a copy drag from it.
//!
//! AppKit asks a destination for its operation synchronously, while the host
//! answers [`Event::Drag`] later with `accept_drag`. Until the host has
//! answered for the current drag, the window accepts any text or files.
#![allow(unsafe_code)] // Objective-C drag session calls need `unsafe`.

use crate::{SEAT, native::Native};
use aegle_platform::{DragData, Error, Event, Result};
use aegle_types::{file_uri, path_from_file_uri};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSDragOperation, NSDraggingInfo, NSDraggingItem, NSImage, NSPasteboard, NSPasteboardItem,
    NSPasteboardTypeFileURL, NSPasteboardTypeString,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};

/// Edge of the invisible image that follows the pointer in a drag we start.
const GRIP: f64 = 24.0;

fn supported(pasteboard: &NSPasteboard) -> bool {
    // SAFETY: static AppKit constants.
    let kinds = NSArray::from_slice(&unsafe { [NSPasteboardTypeFileURL, NSPasteboardTypeString] });
    pasteboard.availableTypeFromArray(&kinds).is_some()
}

/// Files, or else text, from `pasteboard`.
fn read(pasteboard: &NSPasteboard) -> Option<DragData> {
    // SAFETY: static AppKit constants.
    let (file, text) = unsafe { (NSPasteboardTypeFileURL, NSPasteboardTypeString) };
    let files: Vec<_> = pasteboard
        .pasteboardItems()
        .iter()
        .flat_map(|items| items.iter())
        .filter_map(|item| item.stringForType(file))
        .filter_map(|uri| path_from_file_uri(&uri.to_string()))
        .collect();
    if !files.is_empty() {
        return Some(DragData::Files(files));
    }
    let text = pasteboard.stringForType(text)?.to_string();
    Some(DragData::Text(text))
}

impl Native {
    fn drag_operation(&self) -> NSDragOperation {
        if self.drag_active.get() && self.drag_accept.get() != Some(false) {
            NSDragOperation::Copy
        } else {
            NSDragOperation::None
        }
    }

    /// A drag entered the window.
    pub(crate) fn drag_enter(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
        self.drag_active.set(supported(&info.draggingPasteboard()));
        self.drag_accept.set(None);
        self.drag_at.set(None);
        self.drag_over(info)
    }

    /// A drag moved over the window; only motion is reported.
    pub(crate) fn drag_over(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
        let position = self.position(info.draggingLocation());
        if self.drag_active.get() && self.drag_at.replace(Some(position)) != Some(position) {
            self.emit(Event::Drag {
                window: self.id,
                seat: SEAT,
                position,
            });
        }
        self.drag_operation()
    }

    pub(crate) fn drag_exit(&self) {
        if self.drag_active.replace(false) {
            self.emit(Event::DragLeave {
                window: self.id,
                seat: SEAT,
            });
        }
    }

    /// The drag was dropped; whether the data was taken.
    pub(crate) fn drag_drop(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
        let accepted = self.drag_operation() == NSDragOperation::Copy;
        self.drag_active.set(false);
        match accepted.then(|| read(&info.draggingPasteboard())).flatten() {
            Some(data) => {
                self.emit(Event::Drop {
                    window: self.id,
                    seat: SEAT,
                    position: self.position(info.draggingLocation()),
                    data,
                });
                true
            }
            None => {
                self.emit(Event::DragLeave {
                    window: self.id,
                    seat: SEAT,
                });
                false
            }
        }
    }

    /// Begins a copy drag of `data`, authorized by the latest pointer press.
    /// AppKit runs the session from the event loop; this returns at once.
    pub(crate) fn start_drag(&self, mtm: MainThreadMarker, data: &DragData) -> Result<()> {
        let event = self
            .last_mouse
            .borrow()
            .clone()
            .ok_or_else(|| Error::backend("a drag needs a pressed pointer"))?;
        // SAFETY: static AppKit constants.
        let (file, text) = unsafe { (NSPasteboardTypeFileURL, NSPasteboardTypeString) };
        let payloads: Vec<(String, _)> = match data {
            DragData::Text(content) => vec![(content.clone(), text)],
            DragData::Files(files) => files.iter().map(|f| (file_uri(f), file)).collect(),
        };
        let at = self
            .view
            .convertPoint_fromView(event.locationInWindow(), None);
        let frame = NSRect::new(
            NSPoint::new(at.x - GRIP / 2.0, at.y - GRIP / 2.0),
            NSSize::new(GRIP, GRIP),
        );
        let image = NSImage::initWithSize(mtm.alloc(), NSSize::new(GRIP, GRIP));
        let items: Vec<Retained<NSDraggingItem>> = payloads
            .into_iter()
            .map(|(payload, kind)| {
                let item = NSPasteboardItem::new();
                item.setString_forType(&NSString::from_str(&payload), kind);
                let dragging = NSDraggingItem::initWithPasteboardWriter(
                    mtm.alloc(),
                    ProtocolObject::from_ref(&*item),
                );
                // SAFETY: an image is a valid drag image.
                unsafe { dragging.setDraggingFrame_contents(frame, Some(&image)) };
                dragging
            })
            .collect();
        self.view.beginDraggingSessionWithItems_event_source(
            &NSArray::from_retained_slice(&items),
            &event,
            ProtocolObject::from_ref(&*self.view),
        );
        Ok(())
    }
}
