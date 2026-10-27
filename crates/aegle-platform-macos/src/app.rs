//! The process's AppKit application session: activation policy, a minimal
//! menu whose Quit asks the host to close windows, and the application
//! delegate.
#![allow(unsafe_code)]
use crate::shared::Shared;
use aegle_platform::Event;
use objc2::{DefinedClass, MainThreadOnly, define_class, rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSApplicationTerminateReply, NSMenu, NSMenuItem,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSProcessInfo, NSString};
use std::rc::Rc;

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the delegate does
    // not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Rc<Shared>]
    pub(crate) struct AppDelegate;

    impl AppDelegate {
        /// Quits by asking every window to close, as the window's own control
        /// does; the host decides whether it may.
        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&objc2::runtime::AnyObject>) {
            for window in self.ivars().windows.borrow().iter().copied() {
                self.ivars().emit(Event::Close { window });
            }
        }

        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            for window in self.ivars().windows.borrow().iter().copied() {
                self.ivars().emit(Event::Close { window });
            }
            // The host's event loop ends when its last window closes.
            NSApplicationTerminateReply::TerminateCancel
        }
    }

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn terminate_after_last_window(&self, _sender: &NSApplication) -> bool {
            false
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker, shared: Rc<Shared>) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(shared);
        // SAFETY: NSObject's designated initializer, with the ivars set.
        unsafe { objc2::msg_send![super(this), init] }
    }
}

/// A minimal main menu: the application menu with About and Quit. macOS
/// gives windows a menu bar even without one, but then ⌘Q does nothing.
fn menu(mtm: MainThreadMarker, delegate: &AppDelegate) -> Retained<NSMenu> {
    let name = NSProcessInfo::processInfo().processName();
    let main = NSMenu::new(mtm);
    let item = NSMenuItem::new(mtm);
    main.addItem(&item);
    let submenu = NSMenu::new(mtm);
    item.setSubmenu(Some(&submenu));
    // SAFETY: standard menu construction; the delegate outlives the menu.
    unsafe {
        let quit = NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(&format!("Quit {name}")),
            Some(objc2::sel!(quit:)),
            &NSString::from_str("q"),
        );
        quit.setTarget(Some(delegate));
        submenu.addItem(&quit);
    }
    main
}

/// Starts the application: a regular, activating app whose delegate is
/// `shared`'s. Returns the application and the delegate the caller keeps
/// alive; neither is retained by the platform beyond the connection.
pub(crate) fn start(
    mtm: MainThreadMarker,
    shared: Rc<Shared>,
) -> (Retained<NSApplication>, Retained<AppDelegate>) {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = AppDelegate::new(mtm, shared);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    let main = menu(mtm, &delegate);
    app.setMainMenu(Some(&main));
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    app.finishLaunching();
    (app, delegate)
}
