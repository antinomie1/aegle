//! Displays as outputs: `NSScreen` geometry in the contract's top-left
//! desktop space, and screen-parameter changes as [`Event::Outputs`].
#![allow(unsafe_code)] // AppKit notification names and observer removal need `unsafe`.

use crate::shared::Shared;
use aegle_platform::{Event, OutputId, OutputInfo, PixelSize, Point, Rect};
use block2::RcBlock;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSApplicationDidChangeScreenParametersNotification, NSScreen};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSNotificationCenter, NSNumber, NSObjectProtocol,
    NSOperationQueue, NSRect, NSString,
};
use std::{ptr::NonNull, rc::Rc};

/// The height of the primary screen, whose bottom-left corner is the origin
/// of AppKit's y-up desktop space; zero without a screen.
fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map_or(0.0, |screen| screen.frame().size.height)
}

/// `rect`, in AppKit's y-up desktop space, in the contract's top-left space.
pub(crate) fn to_desktop(mtm: MainThreadMarker, rect: NSRect) -> Rect {
    let top = primary_height(mtm) - (rect.origin.y + rect.size.height);
    Rect::new(
        rect.origin.x as f32,
        top as f32,
        rect.size.width as f32,
        rect.size.height as f32,
    )
}

/// The AppKit origin of a window frame whose top-left corner is `point` in
/// the contract's desktop space and whose height is `height`.
pub(crate) fn from_desktop(mtm: MainThreadMarker, point: Point, height: f64) -> (f64, f64) {
    let bottom = primary_height(mtm) - f64::from(point.y) - height;
    (f64::from(point.x), bottom)
}

/// The `CGDirectDisplayID` of `screen`, stable while the display stays connected.
fn display_id(screen: &NSScreen) -> Option<u32> {
    let key = NSString::from_str("NSScreenNumber");
    let number = screen.deviceDescription().objectForKey(&key)?;
    Some(number.downcast::<NSNumber>().ok()?.unsignedIntValue())
}

/// The connected screens, the primary one first. AppKit reports a localized
/// product name, such as "Built-in Retina Display", and no connector name,
/// so that name is the output's name and there is no separate description.
pub(crate) fn outputs(mtm: MainThreadMarker) -> Vec<OutputInfo> {
    NSScreen::screens(mtm)
        .iter()
        .filter_map(|screen| {
            let id = display_id(&screen)?;
            let frame = to_desktop(mtm, screen.frame());
            Some(OutputInfo {
                id: OutputId(u64::from(id)),
                name: screen.localizedName().to_string(),
                description: String::new(),
                position: frame.origin,
                size: PixelSize {
                    width: frame.size.width.round().max(0.0) as u32,
                    height: frame.size.height.round().max(0.0) as u32,
                },
                scale: screen.backingScaleFactor() as f32,
            })
        })
        .collect()
}

/// Reports screen arrangement, resolution and scale changes as one queued
/// [`Event::Outputs`] at a time.
pub(crate) struct ScreenObserver {
    center: Retained<NSNotificationCenter>,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl ScreenObserver {
    pub fn new(shared: Rc<Shared>) -> Self {
        let center = NSNotificationCenter::defaultCenter();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            let queued = shared
                .events
                .borrow()
                .iter()
                .any(|event| matches!(event, Event::Outputs));
            if !queued {
                shared.emit(Event::Outputs);
            }
        });
        // SAFETY: the name is a static AppKit constant; the block runs on the
        // main queue, where it was made, and the observer is removed before
        // the platform goes away.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        };
        Self { center, token }
    }
}

impl Drop for ScreenObserver {
    fn drop(&mut self) {
        // SAFETY: the token came from this center's block-based registration.
        let _: () = unsafe { objc2::msg_send![&self.center, removeObserver: &*self.token] };
    }
}
