//! Event snapshots and focus retain identity across tree edits.

use aegle_core::{
    EventControl, EventPhase, Focus, FocusDirection, FocusError, FocusPolicy, Route, RouteError,
    Tree,
};

#[test]
fn routing_and_focus_follow_retained_lifecycle() {
    use FocusDirection::{Backward, Forward};
    use FocusPolicy::{Focusable, Prune, Skip};
    let mut tree = Tree::new();
    let root = tree.insert(None, Skip).unwrap();
    let first = tree.insert(Some(root), Focusable).unwrap();
    let group = tree.insert(Some(root), Skip).unwrap();
    let last = tree.insert(Some(group), Focusable).unwrap();
    let other = tree.insert(None, Focusable).unwrap();
    let mut route = Route::new();
    route.rebuild(&tree, root, last).unwrap();
    assert_eq!(route.path(), &[root, group, last]);
    let expected = [
        (root, EventPhase::Capture),
        (group, EventPhase::Capture),
        (last, EventPhase::Target),
        (group, EventPhase::Bubble),
        (root, EventPhase::Bubble),
    ];
    assert!(route.iter().map(|s| (s.node, s.phase)).eq(expected));
    tree.reparent(last, Some(root)).unwrap();
    assert_eq!(route.path(), &[root, group, last]);
    assert_eq!(
        route.rebuild(&tree, root, other),
        Err(RouteError::OutsideRoot)
    );
    assert_eq!(route.iter().count(), 0);
    route.rebuild(&tree, root, root).unwrap();
    assert_eq!(route.iter().count(), 1);
    let mut control = EventControl::default();
    control.stop_propagation();
    assert!(!control.is_default_prevented());
    control.prevent_default();
    assert!(control.is_propagation_stopped() && control.is_default_prevented());

    let mut focus = Focus::new();
    let policy = |_, value: &FocusPolicy| *value;
    let advance = |f: &mut Focus, t: &Tree<FocusPolicy>, direction, wrap| {
        f.advance(t, root, direction, wrap, policy).unwrap()
    };
    assert_eq!(
        advance(&mut focus, &tree, Forward, false).current,
        Some(first)
    );
    assert_eq!(
        advance(&mut focus, &tree, Forward, false).current,
        Some(last)
    );
    assert!(!advance(&mut focus, &tree, Forward, false).changed());
    assert_eq!(
        advance(&mut focus, &tree, Forward, true).current,
        Some(first)
    );
    assert_eq!(
        advance(&mut focus, &tree, Backward, true).current,
        Some(last)
    );
    tree.reparent(last, Some(group)).unwrap();
    *tree.get_mut(group).unwrap() = Prune;
    assert_eq!(
        focus.set(&tree, root, Some(last), policy),
        Err(FocusError::NotFocusable)
    );
    assert_eq!(
        advance(&mut focus, &tree, Backward, false).current,
        Some(first)
    );
    tree.remove(first).unwrap();
    assert_eq!(focus.current(&tree), None);
    assert!(!advance(&mut focus, &tree, Forward, true).changed());
    *tree.get_mut(group).unwrap() = Skip;
    focus.set(&tree, root, Some(last), policy).unwrap();
    tree.remove(root).unwrap();
    assert_eq!(focus.current(&tree), None);
    assert_eq!(
        focus.advance(&tree, root, Forward, true, policy),
        Err(RouteError::DeadNode)
    );
    assert_eq!(route.rebuild(&tree, root, last), Err(RouteError::DeadNode));
    assert!(route.path().is_empty());
}
