//! Platform callbacks wake the owner without executing UI code on that thread.
use aegle_access::{
    Event, Mailbox,
    accesskit::{
        Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, NodeId,
        TreeId,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn callbacks_preserve_activation_action_and_deactivation_order() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    let (mut inbox, mut handlers) = Mailbox::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
    });
    std::thread::spawn(move || {
        assert!(handlers.request_initial_tree().is_none());
        handlers.do_action(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: NodeId(8),
            data: None,
        });
        handlers.deactivate_accessibility();
    })
    .join()
    .unwrap();
    assert_eq!(wakes.load(Ordering::Relaxed), 3);
    assert!(matches!(inbox.next_event(), Some(Event::InitialTree)));
    assert!(matches!(
        inbox.next_event(),
        Some(Event::Action(ActionRequest {
            action: Action::Click,
            target_node: NodeId(8),
            ..
        }))
    ));
    assert!(matches!(inbox.next_event(), Some(Event::Deactivate)));
    assert!(inbox.next_event().is_none());
}
