//! Global shortcuts as Carbon hot keys.
//!
//! `RegisterEventHotKey` is the system's own global-shortcut service: it needs
//! no accessibility permission and delivers presses through the application's
//! event loop, which the AppKit loop runs. Keys are matched by position, as
//! [`crate::Trigger::mac_key_code`] maps them; Ctrl, Alt, Shift and the logo
//! key are Control, Option, Shift and Command. macOS 15 refuses hot keys
//! whose only modifiers are Option or Option and Shift, which are then
//! reported unavailable like any other refusal.
#![allow(unsafe_code)] // Carbon is a C API.

use std::{ffi::c_void, sync::Arc};

use super::Sink;
use crate::{Event, Shortcut};

type OsStatus = i32;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHandler = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OsStatus;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[repr(C)]
struct EventTypeSpec {
    class: u32,
    kind: u32,
}

const fn four(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// `kEventClassKeyboard`, `kEventHotKeyPressed`.
const KEYBOARD: u32 = four(b"keyb");
const HOT_KEY_PRESSED: u32 = 5;
/// `kEventParamDirectObject`, `typeEventHotKeyID`.
const DIRECT_OBJECT: u32 = four(b"----");
const HOT_KEY_ID: u32 = four(b"hkid");
/// Marks this crate's hot keys.
const SIGNATURE: u32 = four(b"Aegl");
/// `cmdKey`, `shiftKey`, `optionKey`, `controlKey`.
const COMMAND: u32 = 1 << 8;
const SHIFT: u32 = 1 << 9;
const OPTION: u32 = 1 << 11;
const CONTROL: u32 = 1 << 12;
/// `eventNotHandledErr`, so other handlers see presses that are not ours.
const NOT_HANDLED: OsStatus = -9874;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandler,
        count: usize,
        types: *const EventTypeSpec,
        data: *mut c_void,
        out: *mut EventHandlerRef,
    ) -> OsStatus;
    fn RemoveEventHandler(handler: EventHandlerRef) -> OsStatus;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: EventTargetRef,
        options: u32,
        out: *mut EventHotKeyRef,
    ) -> OsStatus;
    fn UnregisterEventHotKey(key: EventHotKeyRef) -> OsStatus;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        kind: u32,
        actual_kind: *mut u32,
        size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OsStatus;
}

/// What the handler reads: the sink and each registered shortcut's id.
struct Bound {
    sink: Arc<Sink>,
    ids: Vec<String>,
}

/// The registered hot keys and their handler; dropping it unregisters them.
pub(super) struct HotKeys {
    handler: EventHandlerRef,
    keys: Vec<EventHotKeyRef>,
    /// Boxed so the handler's pointer to it stays valid.
    bound: Box<Bound>,
}

extern "C" fn pressed(_call: EventHandlerCallRef, event: EventRef, data: *mut c_void) -> OsStatus {
    let mut key = EventHotKeyId::default();
    // SAFETY: Carbon passes a live hot-key event; the buffer is the size
    // asked for and the null out pointers are optional.
    let status = unsafe {
        GetEventParameter(
            event,
            DIRECT_OBJECT,
            HOT_KEY_ID,
            std::ptr::null_mut(),
            size_of::<EventHotKeyId>(),
            std::ptr::null_mut(),
            (&raw mut key).cast(),
        )
    };
    // SAFETY: `data` is the `Bound` the handler was installed with, which
    // outlives the handler: `HotKeys::drop` removes it first.
    let bound = unsafe { &*data.cast::<Bound>() };
    match bound.ids.get(key.id as usize) {
        Some(id) if status == 0 && key.signature == SIGNATURE => {
            (bound.sink)(Event::Shortcut { id: id.clone() });
            0
        }
        _ => NOT_HANDLED,
    }
}

impl HotKeys {
    /// Registers `shortcuts`, reporting each that cannot be registered as
    /// [`Event::Unavailable`].
    pub fn new(sink: Arc<Sink>, shortcuts: &[Shortcut<'_>]) -> Self {
        let mut bound = Box::new(Bound {
            sink,
            ids: shortcuts.iter().map(|s| s.id.to_owned()).collect(),
        });
        let mut handler = std::ptr::null_mut();
        let spec = EventTypeSpec {
            class: KEYBOARD,
            kind: HOT_KEY_PRESSED,
        };
        // SAFETY: the application target exists for the process; the spec
        // and out pointer are live for the call, and `bound` outlives the
        // handler (see `pressed`).
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                pressed,
                1,
                &spec,
                (&raw mut *bound).cast(),
                &mut handler,
            )
        };
        let mut hot_keys = Self {
            handler: if status == 0 {
                handler
            } else {
                std::ptr::null_mut()
            },
            keys: Vec::new(),
            bound,
        };
        if hot_keys.handler.is_null() {
            (hot_keys.bound.sink)(Event::Unavailable(format!(
                "global shortcuts: InstallEventHandler failed ({status})"
            )));
            return hot_keys;
        }
        for (index, shortcut) in shortcuts.iter().enumerate() {
            let trigger = shortcut.trigger;
            let Some(code) = trigger.mac_key_code() else {
                (hot_keys.bound.sink)(Event::Unavailable(format!(
                    "global shortcut {:?}: the key does not exist on a Mac keyboard",
                    shortcut.id
                )));
                continue;
            };
            let modifiers = [
                (trigger.ctrl, CONTROL),
                (trigger.alt, OPTION),
                (trigger.shift, SHIFT),
                (trigger.logo, COMMAND),
            ]
            .into_iter()
            .filter_map(|(on, bit)| on.then_some(bit))
            .fold(0, |all, bit| all | bit);
            let id = EventHotKeyId {
                signature: SIGNATURE,
                id: index as u32,
            };
            let mut key = std::ptr::null_mut();
            // SAFETY: the application target exists; the out pointer is live.
            let status = unsafe {
                RegisterEventHotKey(
                    code.into(),
                    modifiers,
                    id,
                    GetApplicationEventTarget(),
                    0,
                    &mut key,
                )
            };
            if status == 0 {
                hot_keys.keys.push(key);
            } else {
                (hot_keys.bound.sink)(Event::Unavailable(format!(
                    "global shortcut {:?}: the system refused it ({status}), \
                     usually because another application holds it",
                    shortcut.id
                )));
            }
        }
        hot_keys
    }
}

impl Drop for HotKeys {
    fn drop(&mut self) {
        // SAFETY: each key and the handler came from a successful
        // registration and are released once; the handler goes before the
        // `Bound` it points to.
        unsafe {
            for key in self.keys.drain(..) {
                UnregisterEventHotKey(key);
            }
            if !self.handler.is_null() {
                RemoveEventHandler(self.handler);
            }
        }
    }
}
