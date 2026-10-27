//! The cross-thread wake signal of one connection.
//!
//! A Core Foundation run-loop source is registered in the main run loop's
//! common modes. Any thread can signal it and wake the loop; its perform
//! callback runs on the main thread, queues [`Event::Wake`] and posts an
//! application-defined event, which makes the loop's blocking event fetch
//! return.
#![allow(unsafe_code)]
use crate::shared::Shared;
use aegle_platform::{Event, WakeHandle};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
use objc2_core_foundation::{
    CFIndex, CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, kCFRunLoopCommonModes,
};
use objc2_foundation::NSPoint;
use std::{
    ffi::c_void,
    rc::{Rc, Weak},
};

/// What a perform callback reaches through its context.
struct Context {
    shared: Weak<Shared>,
}

unsafe extern "C-unwind" fn perform(info: *mut c_void) {
    // SAFETY: `info` is the context created below; the source releases it.
    let Some(shared) = (unsafe { &*(info as *const Context) }).shared.upgrade() else {
        return;
    };
    shared.emit(Event::Wake);
    // The callback runs on the main thread, inside the loop's event fetch.
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            NSPoint::ZERO,
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        );
        if let Some(event) = event {
            app.postEvent_atStart(&event, true);
        }
    }
}

unsafe extern "C-unwind" fn release(info: *const c_void) {
    // SAFETY: created by `Box::into_raw` for exactly this source.
    drop(unsafe { Box::from_raw(info as *mut Context) });
}

/// The run loop and source a [`WakeHandle`] signals. Core Foundation keeps
/// both alive after the platform drops, so a late wake is harmless.
struct Signal {
    source: CFRetained<CFRunLoopSource>,
    run_loop: CFRetained<CFRunLoop>,
}

// SAFETY: `CFRunLoopSourceSignal` and `CFRunLoopWakeUp` are documented as
// thread-safe, and the retained objects are never mutated by this struct.
unsafe impl Send for Signal {}
unsafe impl Sync for Signal {}

impl Signal {
    fn signal(&self) {
        self.source.signal();
        self.run_loop.wake_up();
    }
}

/// The registered run-loop source; dropping it unregisters and invalidates.
pub(crate) struct Waker {
    source: Option<CFRetained<CFRunLoopSource>>,
    run_loop: CFRetained<CFRunLoop>,
}

impl Waker {
    /// Creates the source and registers it in the main run loop, on the main
    /// thread. Also returns the platform's signal for other threads.
    pub fn new(shared: &Rc<Shared>) -> Option<(Self, WakeHandle)> {
        let run_loop = CFRunLoop::main()?;
        let context = Box::into_raw(Box::new(Context {
            shared: Rc::downgrade(shared),
        }));
        let mut creation = CFRunLoopSourceContext {
            version: 0,
            info: context.cast(),
            retain: None,
            release: Some(release),
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(perform),
        };
        // SAFETY: a valid context whose info the source releases once; the
        // source is invalidated before it is released.
        let source = unsafe { CFRunLoopSource::new(None, 0 as CFIndex, &mut creation) }?;
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        let signal = Signal {
            source: source.clone(),
            run_loop: run_loop.clone(),
        };
        let wake = WakeHandle::new(move || signal.signal());
        Some((
            Self {
                source: Some(source),
                run_loop,
            },
            wake,
        ))
    }
}

impl Drop for Waker {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            self.run_loop
                .remove_source(Some(&source), unsafe { kCFRunLoopCommonModes });
            source.invalidate();
        }
    }
}
