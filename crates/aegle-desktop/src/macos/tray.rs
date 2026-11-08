//! The tray icon as an `NSStatusItem` in the menu bar.
//!
//! A primary click reports [`Event::TrayActivated`]; a secondary click or a
//! Control-click opens the menu, as the other desktops' trays do. The menu is
//! attached only while it is open, because a status item with a menu opens it
//! on every click and never sends its action.
#![allow(unsafe_code)] // Objective-C subclassing and target-action wiring need `unsafe`.

use std::{cell::RefCell, sync::Arc};

use objc2::{
    AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventMask,
    NSEventModifierFlags, NSEventType, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_core_foundation::CFData;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    kCGColorSpaceSRGB,
};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSSize, NSString};

use super::Sink;
use crate::{Event, Icon, MenuItem, Tray};

/// The icon's height in the menu bar, in points; AppKit's own items use 18.
const ICON_POINTS: f64 = 18.0;

/// The click and menu callbacks' state.
pub(super) struct Callbacks {
    sink: Arc<Sink>,
    item: RefCell<Option<Retained<NSStatusItem>>>,
    menu: RefCell<Option<Retained<NSMenu>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class does
    // not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Callbacks]
    pub(super) struct Target;

    impl Target {
        #[unsafe(method(clicked:))]
        fn clicked(&self, _sender: Option<&AnyObject>) {
            let callbacks = self.ivars();
            let event = NSApplication::sharedApplication(self.mtm()).currentEvent();
            let secondary = event.is_some_and(|event| {
                event.r#type() == NSEventType::RightMouseUp
                    || event.modifierFlags().contains(NSEventModifierFlags::Control)
            });
            let item = callbacks.item.borrow().clone();
            let menu = callbacks.menu.borrow().clone();
            match (secondary, item, menu) {
                (true, Some(item), Some(menu)) => {
                    item.setMenu(Some(&menu));
                    if let Some(button) = item.button(self.mtm()) {
                        // SAFETY: a live button; with a menu attached the
                        // click opens it and returns when it closes.
                        unsafe { button.performClick(None) };
                    }
                    item.setMenu(None);
                }
                (true, ..) => {}
                (false, ..) => (callbacks.sink)(Event::TrayActivated),
            }
        }

        #[unsafe(method(chosen:))]
        fn chosen(&self, sender: &NSMenuItem) {
            let item = sender.tag() as u32;
            (self.ivars().sink)(Event::TrayMenu { item });
        }
    }

    unsafe impl NSObjectProtocol for Target {}
);

/// A shown tray icon.
pub(super) struct StatusItem {
    item: Retained<NSStatusItem>,
    target: Retained<Target>,
}

impl StatusItem {
    /// Adds an item to the menu bar showing `tray`.
    pub fn new(mtm: MainThreadMarker, sink: Arc<Sink>, tray: &Tray<'_>) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        let target = mtm.alloc::<Target>().set_ivars(Callbacks {
            sink,
            item: RefCell::new(Some(item.clone())),
            menu: RefCell::new(None),
        });
        // SAFETY: NSObject's designated initializer, with the ivars set.
        let target: Retained<Target> = unsafe { msg_send![super(target), init] };
        if let Some(button) = item.button(mtm) {
            // SAFETY: the target lives as long as the item, which this owns.
            unsafe {
                button.setTarget(Some(&target));
                button.setAction(Some(sel!(clicked:)));
            }
            button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
        }
        let status = Self { item, target };
        status.update(mtm, tray);
        status
    }

    /// Replaces the icon, tooltip and menu.
    pub fn update(&self, mtm: MainThreadMarker, tray: &Tray<'_>) {
        if let Some(button) = self.item.button(mtm) {
            button.setImage(image(tray.icon).as_deref());
            let tooltip = (!tray.tooltip.is_empty()).then(|| NSString::from_str(tray.tooltip));
            button.setToolTip(tooltip.as_deref());
        }
        let menu = (!tray.menu.is_empty()).then(|| menu(mtm, &self.target, tray.menu));
        *self.target.ivars().menu.borrow_mut() = menu;
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        // Breaks the target's reference to the item.
        self.target.ivars().item.take();
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

/// `icon` as an image 18 points high, from its straight RGBA pixels.
fn image(icon: Icon<'_>) -> Option<Retained<NSImage>> {
    let data = CFData::from_bytes(icon.rgba);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    // SAFETY: a static CoreGraphics constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let (width, height) = (icon.width as usize, icon.height as usize);
    // SAFETY: the provider holds exactly `height` rows of `width * 4` bytes of
    // straight RGBA, which `Desktop::set_tray` checked; no decode array.
    let image = unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            width * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::Last.0),
            Some(&provider),
            std::ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }?;
    let points = NSSize::new(
        ICON_POINTS * f64::from(icon.width) / f64::from(icon.height),
        ICON_POINTS,
    );
    Some(NSImage::initWithCGImage_size(
        NSImage::alloc(),
        &image,
        points,
    ))
}

/// A menu of `items` whose commands report to `target`.
fn menu(mtm: MainThreadMarker, target: &Target, items: &[MenuItem<'_>]) -> Retained<NSMenu> {
    let built = NSMenu::new(mtm);
    // Items are enabled as the application says, not by responder lookup.
    built.setAutoenablesItems(false);
    for entry in items {
        let item = match *entry {
            MenuItem::Separator => NSMenuItem::separatorItem(mtm),
            MenuItem::Item {
                id,
                label,
                enabled,
                checked,
            } => {
                // SAFETY: standard menu item construction; the target lives
                // as long as the status item that shows the menu.
                let item = unsafe {
                    let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                        mtm.alloc(),
                        &NSString::from_str(label),
                        Some(sel!(chosen:)),
                        &NSString::from_str(""),
                    );
                    item.setTarget(Some(target));
                    item
                };
                item.setTag(id as isize);
                item.setEnabled(enabled);
                if let Some(checked) = checked {
                    item.setState(match checked {
                        true => NSControlStateValueOn,
                        false => NSControlStateValueOff,
                    });
                }
                item
            }
            MenuItem::Submenu { label, items } => {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(label));
                let submenu = menu(mtm, target, items);
                submenu.setTitle(&NSString::from_str(label));
                item.setSubmenu(Some(&submenu));
                item
            }
        };
        built.addItem(&item);
    }
    built
}
