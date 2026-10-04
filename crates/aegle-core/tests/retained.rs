//! Retained-state integration contracts.
use aegle_core::{Dirty, Tree, TreeError};

#[test]
fn removal_reuses_storage_without_reviving_handles() {
    let mut tree = Tree::new();
    let root = tree.insert(None, 1).unwrap();
    let branch = tree.insert(Some(root), 2).unwrap();
    let leaf = tree.insert(Some(branch), 3).unwrap();
    let sibling = tree.insert(Some(root), 4).unwrap();
    let mut removed = Vec::new();
    tree.remove_with(branch, |id, value| removed.push((id, value)))
        .unwrap();
    assert_eq!(removed, [(leaf, 3), (branch, 2)]);
    assert_eq!(tree.children(root).unwrap().collect::<Vec<_>>(), [sibling]);
    let replacement = tree.insert(Some(root), 5).unwrap();
    assert_ne!(replacement, branch);
    assert_eq!(tree.get(branch), None);
    assert_eq!(tree.get(leaf), None);
    assert_eq!(tree.get(replacement), Some(&5));
    tree.remove(root).unwrap();
    assert!(tree.is_empty());
}

#[test]
fn edits_preserve_acyclic_order_and_propagate_only_layout() {
    let mut tree = Tree::new();
    let root = tree.insert(None, ()).unwrap();
    let left = tree.insert(Some(root), ()).unwrap();
    let right = tree.insert(Some(root), ()).unwrap();
    let child = tree.insert(Some(left), ()).unwrap();
    for id in [root, left, right, child] {
        tree.clear_dirty(id, Dirty::ALL).unwrap();
    }
    tree.mark_dirty(child, Dirty::PAINT).unwrap();
    assert!(tree.dirty(root).unwrap().is_empty());
    assert_eq!(tree.reparent(root, Some(child)), Err(TreeError::Cycle));
    assert_eq!(tree.parent(root).unwrap(), None);
    tree.reparent(child, Some(right)).unwrap();
    assert_eq!(tree.parent(child).unwrap(), Some(right));
    assert_eq!(tree.children(left).unwrap().len(), 0);
    for id in [root, left, right] {
        assert!(tree.dirty(id).unwrap().intersects(Dirty::LAYOUT));
    }
}

#[test]
fn deeply_nested_removal_does_not_use_the_call_stack() {
    let mut tree = Tree::new();
    let root = tree.insert(None, ()).unwrap();
    let mut leaf = root;
    for _ in 0..2_000 {
        leaf = tree.insert(Some(leaf), ()).unwrap();
    }
    tree.remove(root).unwrap();
    assert!(tree.is_empty());
    assert_eq!(tree.get(leaf), None);
}
