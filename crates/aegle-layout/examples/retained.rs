//! Inspect retained layout and cache reuse without a window or GPU.
use aegle_core::{Dirty, Tree};
use aegle_layout::{AvailableSpace, Dimension, LayoutNode, Size, Style, compute};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tree = Tree::with_capacity(2);
    let root = tree.insert(
        None,
        LayoutNode::with_style(
            Style {
                size: Size {
                    width: Dimension::length(320.0),
                    height: Dimension::length(80.0),
                },
                ..Style::default()
            },
            Size::ZERO,
        ),
    )?;
    let label = tree.insert(
        Some(root),
        LayoutNode::new(Size {
            width: 80.0,
            height: 20.0,
        }),
    )?;
    let available = Size {
        width: AvailableSpace::Definite(320.0),
        height: AvailableSpace::Definite(80.0),
    };
    let mut calls = 0;
    let mut measure = |_, intrinsic: &mut Size<f32>, _: Size<Option<f32>>, _| {
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
