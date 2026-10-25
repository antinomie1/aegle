//! Retained-state integration contracts.
use aegle_core::{Dirty, NodeId, Tree};
use aegle_layout::*;
use aegle_types::Size;
use std::cell::Cell;

fn sized(width: f32, height: f32) -> LayoutStyle {
    let mut style = LayoutStyle::default();
    style.set_width(width).set_height(height);
    style
}

#[test]
fn retained_layout_reuses_measurement_until_intrinsic_or_structure_changes() {
    let mut tree = Tree::new();
    let mut root_style = sized(200.0, 100.0);
    root_style.set_align_items(Some(Align::Start));
    let root = tree
        .insert(None, LayoutNode::with_style(root_style, Size::default()))
        .unwrap();
    let child = tree
        .insert(Some(root), LayoutNode::new(Size::new(40.0, 20.0)))
        .unwrap();
    let calls = Cell::new(0);
    let mut measure = |_: NodeId, size: &mut Size, known: Known, _: Space| {
        calls.set(calls.get() + 1);
        Size::new(
            known.width.unwrap_or(size.width),
            known.height.unwrap_or(size.height),
        )
    };
    compute(&mut tree, root, Space::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(
        tree.get(child).unwrap().bounds().size,
        Size::new(40.0, 20.0)
    );
    let first_calls = calls.get();
    assert!(first_calls > 0);
    tree.mark_dirty(child, Dirty::PAINT).unwrap();
    compute(&mut tree, root, Space::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(calls.get(), first_calls);
    tree.update(child, Dirty::LAYOUT, |node| node.context.width = 70.0)
        .unwrap();
    compute(&mut tree, root, Space::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(tree.get(child).unwrap().bounds().size.width, 70.0);
    assert!(calls.get() > first_calls);
    tree.remove(child).unwrap();
    compute(&mut tree, root, Space::MAX_CONTENT, &mut measure).unwrap();
    assert_eq!(tree.children(root).unwrap().len(), 0);
}

#[test]
fn hiding_and_showing_a_subtree_restores_its_geometry() {
    let mut tree = Tree::new();
    let root = tree.insert(None, LayoutNode::new(())).unwrap();
    let row = tree.insert(Some(root), LayoutNode::new(())).unwrap();
    let child = tree
        .insert(Some(row), LayoutNode::with_style(sized(30.0, 20.0), ()))
        .unwrap();
    let measure = |_, _: &mut (), _, _| Size::default();
    compute(&mut tree, root, Space::MAX_CONTENT, measure).unwrap();
    let mut hidden = LayoutStyle::default();
    hidden.set_display(Display::None);
    set_style(&mut tree, row, hidden).unwrap();
    compute(&mut tree, root, Space::MAX_CONTENT, measure).unwrap();
    assert!(tree.get(child).unwrap().bounds().is_empty());
    set_style(&mut tree, row, LayoutStyle::default()).unwrap();
    compute(&mut tree, root, Space::MAX_CONTENT, measure).unwrap();
    assert_eq!(
        tree.get(child).unwrap().bounds().size,
        Size::new(30.0, 20.0)
    );
}

#[test]
fn styles_read_back_what_was_set_and_reject_invalid_values() {
    let mut style = LayoutStyle::row();
    style
        .set_padding(Insets::new(1.0, Length::Percent(5.0), 3.0, 0.0))
        .set_overflow(Overflow::Hidden, Overflow::Scroll)
        .set_layout_direction(LayoutDirection::Rtl)
        .set_absolute(Some(Insets::all(0.0)));
    assert_eq!(style.direction(), Direction::Row);
    assert_eq!(
        style.padding(),
        Insets::new(1.0, Length::Percent(5.0), 3.0, 0.0)
    );
    assert_eq!(style.overflow(), (Overflow::Hidden, Overflow::Scroll));
    assert_eq!(style.layout_direction(), LayoutDirection::Rtl);
    assert!(style.is_absolute());
    assert_eq!(style.display(), Display::Flex);
    for invalid in [
        |s: &mut LayoutStyle| {
            s.set_width(-1.0);
        },
        |s: &mut LayoutStyle| {
            s.set_grow(f32::NAN);
        },
        |s: &mut LayoutStyle| {
            s.set_padding(Insets::all(Length::Auto));
        },
    ] {
        let mut style = LayoutStyle::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| invalid(&mut style)));
        assert!(result.is_err());
    }
}
