//! `NSTextInputClient` on the content view: AppKit's queries answered from the
//! focused editor's [`Composition`], and its edits reported as
//! [`aegle_platform::ImeUpdate`]s.
#![allow(unsafe_code)] // The protocol methods take raw out-parameters.

use crate::{ime::KeyOutcome, ime_text::NOT_FOUND, native::Native};
use aegle_platform::ImeEvent;
use objc2_foundation::{NSPoint, NSRange, NSRect, NSSize};
use std::ops::Range;

/// The UTF-16 range of `range`, or `None` for `NSNotFound`.
fn utf16_range(range: NSRange) -> Option<Range<usize>> {
    (range.location != NOT_FOUND).then(|| {
        let location = range.location;
        location..location.saturating_add(range.length)
    })
}

/// The `NSRange` of a UTF-16 range.
fn ns_range(range: Range<usize>) -> NSRange {
    NSRange::new(range.start, range.end - range.start)
}

impl Native {
    pub(crate) fn marked_range(&self) -> NSRange {
        self.ime
            .borrow()
            .marked_range()
            .map_or(NSRange::new(NOT_FOUND, 0), ns_range)
    }

    pub(crate) fn selected_range(&self) -> NSRange {
        ns_range(self.ime.borrow().selected_range())
    }

    /// The document text of `range` and the range actually covered.
    pub(crate) fn substring(&self, range: NSRange) -> Option<(String, NSRange)> {
        let (text, covered) = self.ime.borrow().substring(utf16_range(range)?)?;
        Some((text, ns_range(covered)))
    }

    fn update(&self, update: Option<aegle_platform::ImeUpdate>) {
        if let Some(update) = update {
            self.emit_ime(ImeEvent::Update(update));
        }
    }

    /// `setMarkedText:selectedRange:replacementRange:`
    pub(crate) fn set_marked(&self, text: String, selected: NSRange, replacement: NSRange) {
        let selected = utf16_range(selected).unwrap_or(0..0);
        let update = self
            .ime
            .borrow_mut()
            .set_marked(&text, selected, utf16_range(replacement));
        self.update(update);
    }

    /// `insertText:replacementRange:`
    pub(crate) fn insert(&self, text: String, replacement: NSRange) {
        let update = self
            .ime
            .borrow_mut()
            .insert(&text, utf16_range(replacement));
        self.update(update);
    }

    /// `unmarkText`
    pub(crate) fn unmark(&self) {
        let update = self.ime.borrow_mut().unmark();
        self.update(update);
    }

    /// Starts recording what one key press does to the input client.
    pub(crate) fn begin_key(&self) {
        self.ime.borrow_mut().key_down();
    }

    /// Ends the key press and returns what the client learned.
    pub(crate) fn end_key(&self) -> KeyOutcome {
        self.ime.borrow_mut().key_up()
    }

    pub(crate) fn ime_enabled(&self) -> bool {
        self.ime.borrow().is_focused()
    }

    /// The caret area in screen coordinates, where candidates appear.
    pub(crate) fn caret_screen_rect(&self) -> NSRect {
        let caret = self.caret.get();
        let height = self.view.bounds().size.height;
        let rect = NSRect::new(
            NSPoint::new(
                f64::from(caret.origin.x),
                height - f64::from(caret.origin.y) - f64::from(caret.size.height),
            ),
            NSSize::new(f64::from(caret.size.width), f64::from(caret.size.height)),
        );
        let rect = self.view.convertRect_toView(rect, None);
        self.window.convertRectToScreen(rect)
    }
}
