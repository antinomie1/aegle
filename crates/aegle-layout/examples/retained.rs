//! Inspect retained layout and cache reuse without a window or GPU.
use aegle_core::{Dirty, Tree};
use aegle_layout::{Known, LayoutNode, LayoutStyle, Space, compute};
use aegle_types::Size;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tree = Tree::with_capacity(2);
    let mut style = LayoutStyle::default();
    style.set_width(320.0).set_height(80.0);
    let root = tree.insert(None, LayoutNode::with_style(style, Size::default()))?;
    let label = tree.insert(Some(root), LayoutNode::new(Size::new(80.0, 20.0)))?;
    let available = Space::definite(Size::new(320.0, 80.0));
    let mut calls = 0;
    let mut measure = |_, intrinsic: &mut Size, _: Known, _| {
        calls += 1;
        *intrinsic
    };
    compute(&mut tree, root, available, &mut measure)?;
    compute(&mut tree, root, available, &mut measure)?;
    tree.update(label, Dirty::LAYOUT, |node| node.context.width = 120.0)?;
    compute(&mut tree, root, available, &mut measure)?;
    println!("label: {:?}", tree.get(label).unwrap().bounds());
    println!("measurements across three passes: {calls}");
    println!("reserved tree storage: {} bytes", tree.allocated_bytes());
    Ok(())
}
