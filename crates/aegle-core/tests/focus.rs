//! Focus retains identity across tree edits.

use aegle_core::{Focus, FocusDirection, FocusError, FocusPolicy, Tree};

#[test]
fn focus_follows_retained_lifecycle() {
    use FocusDirection::{Backward, Forward};
    use FocusPolicy::{Focusable, Prune, Skip};
    let mut tree = Tree::new();
    let root = tree.insert(None, Skip).unwrap();
    let first = tree.insert(Some(root), Focusable).unwrap();
    let group = tree.insert(Some(root), Skip).unwrap();
    let last = tree.insert(Some(group), Focusable).unwrap();
    let other = tree.insert(None, Focusable).unwrap();
    let mut focus = Focus::new();
    let policy = |_, value: &FocusPolicy| *value;
    let advance = |f: &mut Focus, t: &Tree<FocusPolicy>, direction, wrap| {
        f.advance(t, root, direction, wrap, policy).unwrap()
    };
    assert_eq!(
        focus.set(&tree, root, Some(other), policy),
        Err(FocusError::OutsideRoot)
    );
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
        Err(FocusError::DeadNode)
    );
}
