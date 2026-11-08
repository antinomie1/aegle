//! Notifications through `UNUserNotificationCenter`.
//!
//! The center serves application bundles only: a process without a bundle
//! identifier cannot even reach it, so `notify` then reports
//! [`Event::Unavailable`]. Every notification first asks for authorization,
//! which the system answers at once after the user's first decision, and is
//! delivered from that answer; a refusal is reported as unavailable. The
//! center's delegate shows notifications while the application is in front,
//! reports a click on the body as the action `"default"` and a button as its
//! key, then [`Event::NotificationClosed`]; dismissing reports only the close.
#![allow(unsafe_code)] // Objective-C subclassing and the center's raw error pointers.

use std::sync::Arc;

use block2::RcBlock;
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{Bool, NSObject, ProtocolObject},
};
use objc2_foundation::{NSArray, NSBundle, NSError, NSObjectProtocol, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationDefaultActionIdentifier, UNNotificationDismissActionIdentifier,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::Sink;
use crate::{Event, Notification};

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class does
    // not implement `Drop`. The center may call it on any thread; its only
    // state is the thread-safe sink.
    #[unsafe(super(NSObject))]
    #[ivars = Callbacks]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // The system hides notifications of the frontmost application
            // unless told to show them.
            handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            let ours = identifier
                .strip_prefix(&self.ivars().prefix)
                .and_then(|id| id.parse().ok());
            if let Some(notification) = ours {
                let sink = &self.ivars().sink;
                let action = response.actionIdentifier();
                // SAFETY: static UserNotifications constants.
                let (default, dismiss) = unsafe {
                    (
                        UNNotificationDefaultActionIdentifier,
                        UNNotificationDismissActionIdentifier,
                    )
                };
                if *action != *dismiss {
                    let action = match *action == *default {
                        true => "default".to_owned(),
                        false => action.to_string(),
                    };
                    sink(Event::NotificationAction {
                        notification,
                        action,
                    });
                }
                sink(Event::NotificationClosed { notification });
            }
            handler.call(());
        }
    }
);

/// A notification's buttons as `(key, label)`.
type Actions = Vec<(String, String)>;

/// What the delegate reports with.
struct Callbacks {
    sink: Arc<Sink>,
    /// Starts the identifiers of this process's requests. The center keeps
    /// notifications across launches, and a request reusing a delivered
    /// identifier replaces it without showing, so ids alone would collide.
    prefix: String,
}

/// The application's notification center and delegate.
pub(super) struct Notifier {
    center: Retained<UNUserNotificationCenter>,
    delegate: Retained<Delegate>,
    /// Registered categories, one per distinct set of actions.
    categories: Vec<(Actions, Retained<UNNotificationCategory>)>,
}

impl Notifier {
    /// The center of this application, or why there is none.
    pub fn new(sink: Arc<Sink>) -> Result<Self, String> {
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            return Err(
                "desktop notifications need an application bundle with an identifier".into(),
            );
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let launched = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let prefix = format!("{}.{launched}.", std::process::id());
        let delegate = Delegate::alloc().set_ivars(Callbacks { sink, prefix });
        // SAFETY: NSObject's designated initializer, with the ivars set.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Ok(Self {
            center,
            delegate,
            categories: Vec::new(),
        })
    }

    /// The category carrying `actions`, registered on first use. Every
    /// category reports dismissals.
    fn category(&mut self, actions: &[(&str, &str)]) -> Retained<NSString> {
        let key: Actions = (actions.iter())
            .map(|&(id, label)| (id.to_owned(), label.to_owned()))
            .collect();
        if let Some((_, category)) = self.categories.iter().find(|(known, _)| *known == key) {
            return category.identifier();
        }
        let identifier = NSString::from_str(&format!("aegle.{}", self.categories.len()));
        let buttons: Vec<_> = (actions.iter())
            .map(|&(id, label)| {
                UNNotificationAction::actionWithIdentifier_title_options(
                    &NSString::from_str(id),
                    &NSString::from_str(label),
                    UNNotificationActionOptions::empty(),
                )
            })
            .collect();
        let category =
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &identifier,
                &NSArray::from_retained_slice(&buttons),
                &NSArray::new(),
                UNNotificationCategoryOptions::CustomDismissAction,
            );
        self.categories.push((key, category));
        let all: Vec<_> = self
            .categories
            .iter()
            .map(|(_, category)| category.clone())
            .collect();
        self.center
            .setNotificationCategories(&NSSet::from_retained_slice(&all));
        identifier
    }

    /// Shows `notification` as `id` once authorized, reporting a refusal or
    /// failure to `sink`.
    pub fn notify(&mut self, sink: &Arc<Sink>, id: u32, notification: &Notification<'_>) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(notification.summary));
        content.setBody(&NSString::from_str(notification.body));
        content.setCategoryIdentifier(&self.category(notification.actions));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!("{}{id}", self.delegate.ivars().prefix)),
            &content,
            None,
        );
        // The answer arrives on another thread; the request and center are
        // thread-safe, so the block may carry them there.
        let sink = sink.clone();
        let center = self.center.clone();
        let answered = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            if !granted.as_bool() {
                // SAFETY: the center passes null or a live error.
                let reason = unsafe { error.as_ref() }
                    .map(|error| format!(": {}", error.localizedDescription()))
                    .unwrap_or_default();
                sink(Event::Unavailable(format!(
                    "desktop notifications are not allowed for this application{reason}"
                )));
                return;
            }
            let sink = sink.clone();
            let added = RcBlock::new(move |error: *mut NSError| {
                // SAFETY: the center passes null or a live error.
                if let Some(error) = unsafe { error.as_ref() } {
                    sink(Event::Unavailable(format!(
                        "desktop notification {id}: {}",
                        error.localizedDescription()
                    )));
                }
            });
            center.addNotificationRequest_withCompletionHandler(&request, Some(&added));
        });
        self.center
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert,
                &answered,
            );
    }
}
