//! The content view: AppKit's callbacks forwarded to the window's [`Native`].
//!
//! One class is the view, the window delegate, the text input client and the
//! drag source and destination, so a window needs a single Objective-C object
//! that holds nothing strongly: its state is the `Weak` ivar. Every method
//! only translates arguments and calls into `Native`.
#![allow(unsafe_code)] // Objective-C method calls and superclass dispatch need `unsafe`.

use crate::native::Native;
use aegle_platform::Event;
use objc2::{
    AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
};
use objc2_app_kit::{
    NSDragOperation, NSDraggingContext, NSDraggingDestination, NSDraggingInfo, NSDraggingSession,
    NSDraggingSource, NSEvent, NSTextInputClient, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSWindow, NSWindowDelegate, NSWindowOcclusionState,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSAttributedString, NSNotification, NSObjectProtocol, NSPoint,
    NSRange, NSRect, NSString, NSUInteger,
};
use std::rc::{Rc, Weak};

/// The text of a string or attributed string argument.
fn text_of(object: &AnyObject) -> String {
    if let Some(attributed) = object.downcast_ref::<NSAttributedString>() {
        attributed.string().to_string()
    } else if let Some(string) = object.downcast_ref::<NSString>() {
        string.to_string()
    } else {
        String::new()
    }
}

define_class!(
    // SAFETY: NSView has no subclassing requirements beyond those upheld by
    // the overrides below, and the class does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Weak<Native>]
    pub(crate) struct View;

    impl View {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if let Some(native) = self.native() {
                native.draw_rect();
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // SAFETY: the superclass method takes no arguments; ours replaces
            // only the areas this view installed.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
            for area in self.trackingAreas().iter() {
                self.removeTrackingArea(&area);
            }
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::ActiveInActiveApp
                | NSTrackingAreaOptions::InVisibleRect;
            // SAFETY: the area tracks this live view and outlives its call.
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    self.bounds(),
                    options,
                    Some(self),
                    None,
                )
            };
            self.addTrackingArea(&area);
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            if let Some(native) = self.native() {
                self.addCursorRect_cursor(self.bounds(), &crate::window::cursor(native.cursor.get()));
            }
        }

        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            if let Some(native) = self.native() {
                native.refresh();
            }
        }

        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn view_did_change_effective_appearance(&self) {
            if let Some(native) = self.native() {
                let current = crate::preferences::read(self.mtm());
                native.shared.refresh_preferences(current);
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.with(|native| native.mouse_down(event));
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.with(|native| native.mouse_down(event));
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.with(|native| native.mouse_down(event));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.with(|native| native.mouse_up(event));
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.with(|native| native.mouse_up(event));
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.with(|native| native.mouse_up(event));
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.with(|native| native.mouse_move(event));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.with(|native| native.mouse_move(event));
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.with(|native| native.mouse_move(event));
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.with(|native| native.mouse_move(event));
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            self.with(|native| native.mouse_enter(event));
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, event: &NSEvent) {
            self.with(|native| native.mouse_exit(event));
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.with(|native| native.scroll(event));
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            self.with(|native| native.key_down(event));
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            self.with(|native| native.key_up(event));
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            self.with(|native| native.flags_changed(event));
        }
    }

    unsafe impl NSObjectProtocol for View {}

    unsafe impl NSTextInputClient for View {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, replacement: NSRange) {
            self.with(|native| native.insert(text_of(string), replacement));
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: objc2::runtime::Sel) {
            // The key's command is reported as the key itself.
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, string: &AnyObject, selected: NSRange, replacement: NSRange) {
            self.with(|native| native.set_marked(text_of(string), selected, replacement));
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            self.with(Native::unmark);
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            self.native().map_or(NSRange::new(0, 0), |n| n.selected_range())
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            self.native()
                .map_or(NSRange::new(crate::ime_text::NOT_FOUND, 0), |n| {
                    n.marked_range()
                })
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            self.native().is_some_and(|native| native.composing())
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(
            &self,
            range: NSRange,
            actual: *mut NSRange,
        ) -> Option<Retained<NSAttributedString>> {
            let found = self.native().and_then(|native| native.substring(range));
            found.map(|(text, covered)| {
                if !actual.is_null() {
                    // SAFETY: AppKit passes null or a valid output range.
                    unsafe { *actual = covered };
                }
                NSAttributedString::initWithString(
                    NSAttributedString::alloc(),
                    &NSString::from_str(&text),
                )
            })
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes_for_marked_text(&self) -> Retained<NSArray<NSString>> {
            NSArray::new()
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect(&self, _range: NSRange, _actual: *mut NSRange) -> NSRect {
            self.native().map_or(NSRect::ZERO, |native| native.caret_screen_rect())
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index_for_point(&self, _point: NSPoint) -> NSUInteger {
            crate::ime_text::NOT_FOUND
        }
    }

    unsafe impl NSDraggingDestination for View {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            self.native().map_or(NSDragOperation::None, |n| n.drag_enter(info))
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            self.native().map_or(NSDragOperation::None, |n| n.drag_over(info))
        }

        #[unsafe(method(draggingExited:))]
        fn dragging_exited(&self, _info: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            self.with(Native::drag_exit);
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            self.native().is_some_and(|native| native.drag_drop(info))
        }
    }

    unsafe impl NSDraggingSource for View {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }
    }

    unsafe impl NSWindowDelegate for View {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _window: &NSWindow) -> bool {
            // The host decides whether to remove the window.
            self.with(|native| native.emit(Event::Close { window: native.id }));
            false
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            self.with(Native::refresh);
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, _notification: &NSNotification) {
            self.with(Native::refresh);
        }

        #[unsafe(method(windowDidChangeScreen:))]
        fn window_did_change_screen(&self, _notification: &NSNotification) {
            self.with(Native::refresh);
        }

        #[unsafe(method(windowDidMiniaturize:))]
        fn window_did_miniaturize(&self, _notification: &NSNotification) {
            self.with(Native::refresh);
        }

        #[unsafe(method(windowDidDeminiaturize:))]
        fn window_did_deminiaturize(&self, _notification: &NSNotification) {
            self.with(|native| {
                native.dirty.set(true);
                native.refresh();
            });
        }

        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            self.with(|native| native.key_changed(true));
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.with(|native| native.key_changed(false));
        }

        #[unsafe(method(windowDidChangeOcclusionState:))]
        fn window_did_change_occlusion_state(&self, _notification: &NSNotification) {
            self.with(|native| {
                let visible = native.window.occlusionState().contains(NSWindowOcclusionState::Visible);
                native.occluded.set(!visible);
                native.dirty.set(native.dirty.get() || visible);
            });
        }
    }
);

impl View {
    /// A view of `frame` that reports to `native`.
    pub(crate) fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        native: Weak<Native>,
    ) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(native);
        // SAFETY: `initWithFrame:` is NSView's designated initializer.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// The window state, while the window is registered with the platform.
    fn native(&self) -> Option<Rc<Native>> {
        self.ivars()
            .upgrade()
            .filter(|native| native.registered.get())
    }

    fn with(&self, f: impl FnOnce(&Native)) {
        if let Some(native) = self.native() {
            f(&native);
        }
    }
}
