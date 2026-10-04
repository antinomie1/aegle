//! Retained-state integration contracts.
use aegle_core::{Dirty, NodeId, Tree};
use aegle_layout::*;
use std::cell::Cell;
use taffy::prelude::TaffyMaxContent;

#[test]
fn retained_layout_reuses_measurement_until_intrinsic_or_structure_changes() {
    let mut tree = Tree::new();
    let root = tree
        .insert(
            None,
            LayoutNode::with_style(
                Style {
                    size: Size {
                        width: Dimension::length(200.0),
                        height: Dimension::length(100.0),
                    },
                    align_items: Some(taffy::AlignItems::START),
                    ..Style::default()
                },
                Size::ZERO,
            ),
        )
        .unwrap();
    let child = tree
        .insert(
            Some(root),
            LayoutNode::new(Size {
                width: 40.0,
                height: 20.0,
            }),
        )
        .unwrap();
    let calls = Cell::new(0);
    let mut measure =
        |_: NodeId, size: &mut Size<f32>, known: Size<Option<f32>>, _: Size<AvailableSpace>| {
            calls.set(calls.get() + 1);
            Size {
                width: known.width.unwrap_or(size.width),
                height: known.height.unwrap_or(size.height),
            }
        };
    compute(&mut tree, root, Size::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(
        tree.get(child).unwrap().bounds().size,
        aegle_types::Size::new(40.0, 20.0)
    );
    let first_calls = calls.get();
    assert!(first_calls > 0);
    tree.mark_dirty(child, Dirty::PAINT).unwrap();
    compute(&mut tree, root, Size::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(calls.get(), first_calls);
    tree.update(child, Dirty::LAYOUT, |node| node.context.width = 70.0)
        .unwrap();
    compute(&mut tree, root, Size::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(tree.get(child).unwrap().bounds().size.width, 70.0);
    assert!(calls.get() > first_calls);
    tree.remove(child).unwrap();
    compute(&mut tree, root, Size::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(tree.children(root).unwrap().len(), 0);
}

#[test]
fn hiding_and_showing_a_subtree_restores_its_geometry() {
    let mut tree = Tree::new();
    let root = tree.insert(None, LayoutNode::new(())).unwrap();
    let row = tree.insert(Some(root), LayoutNode::new(())).unwrap();
    let child = tree
        .insert(
            Some(row),
            LayoutNode::with_style(
                Style {
                    size: Size {
                        width: Dimension::length(30.0),
                        height: Dimension::length(20.0),
                    },
                    ..Style::default()
                },
                (),
            ),
        )
        .unwrap();
    let measure = |_, _: &mut (), _, _| Size::ZERO;
    compute(&mut tree, root, Size::MAX_CONTENT, measure).unwrap();
    set_style(
        &mut tree,
        row,
        Style {
            display: Display::None,
            ..Style::default()
        },
    )
    .unwrap();
    compute(&mut tree, root, Size::MAX_CONTENT, measure).unwrap();
    assert!(tree.get(child).unwrap().bounds().is_empty());
    set_style(&mut tree, row, Style::default()).unwrap();
    compute(&mut tree, root, Size::MAX_CONTENT, measure).unwrap();
    assert_eq!(
        tree.get(child).unwrap().bounds().size,
        aegle_types::Size::new(30.0, 20.0)
    );
}
