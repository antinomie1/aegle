//! Retained layout and software pixels; shapes only, without a window or text.
use aegle_core::{Dirty, NodeId, Tree};
use aegle_layout::{
    AvailableSpace, Dimension, Edges, FlexDirection, LayoutNode, Size, Style, compute,
};
use aegle_render_software::{Renderer, Surface};
use aegle_scene::{Affine, Color, Point, Rect, RoundedRect, Scene};
use std::{error::Error, fs::File, io::BufWriter, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Nodes = Tree<LayoutNode<Visual>>;
const WIDTH: u32 = 480;
const HEIGHT: u32 = 300;

struct Visual {
    color: Color,
    radius: f32,
    mark: bool,
    painted_size: Size<f32>,
    scene: Scene,
}

fn style([width, height]: [u16; 2], column: bool, padding: u16, gap: u16) -> Style {
    Style {
        size: Size {
            width: Dimension::length(f32::from(width)),
            height: Dimension::length(f32::from(height)),
        },
        flex_direction: if column {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        },
        padding: Edges::length(padding),
        gap: Size::length(gap),
        ..Style::default()
    }
}

fn add(
    tree: &mut Nodes,
    parent: Option<NodeId>,
    style: Style,
    color: Color,
    radius: f32,
) -> Result<NodeId> {
    Ok(tree.insert(
        parent,
        LayoutNode::with_style(
            style,
            Visual {
                color,
                radius,
                mark: false,
                painted_size: Size::ZERO,
                scene: Scene::default(),
            },
        ),
    )?)
}

fn paint(
    tree: &mut Nodes,
    root: NodeId,
    renderer: &mut Renderer,
    surface: &mut Surface<'_>,
    stack: &mut Vec<(NodeId, Point)>,
) -> Result<(usize, usize)> {
    let mut frame = renderer.begin_frame(surface, Color::rgb(237, 240, 244));
    let (mut rebuilt, mut bytes) = (0, 0);
    stack.clear();
    stack.push((root, Point::default()));
    while let Some((id, parent_origin)) = stack.pop() {
        let dirty = tree.dirty(id)?.intersects(Dirty::PAINT);
        let node = tree.get_mut(id).unwrap();
        let bounds = node.bounds();
        let size = Size {
            width: bounds.size.width,
            height: bounds.size.height,
        };
        let visual = &mut node.context;
        if dirty || visual.painted_size != size {
            let mut builder = std::mem::take(&mut visual.scene).into_builder();
            builder.clear();
            let shape =
                RoundedRect::new(Rect::new(0.0, 0.0, size.width, size.height), visual.radius)?;
            if visual.color != Color::TRANSPARENT {
                builder.fill(shape, visual.color)?;
            }
            if visual.mark {
                builder
                    .push_clip(shape)?
                    .push_transform(Affine::translation(8.0, 8.0)?)?;
                builder.fill(
                    RoundedRect::new(Rect::new(0.0, 0.0, 22.0, 22.0), 5.0)?,
                    Color::rgba(255, 255, 255, 210),
                )?;
                builder.pop()?.pop()?;
            }
            visual.scene = builder.finish()?;
            visual.painted_size = size;
            rebuilt += 1;
        }
        let origin = Point::new(
            parent_origin.x + bounds.origin.x,
            parent_origin.y + bounds.origin.y,
        );
        bytes += visual.scene.allocated_bytes();
        frame.draw(&visual.scene, Affine::translation(origin.x, origin.y)?)?;
        tree.clear_dirty(id, Dirty::PAINT)?;
        stack.extend(tree.children(id)?.rev().map(|child| (child, origin)));
    }
    Ok((rebuilt, bytes))
}

fn main() -> Result<()> {
    let mut tree = Nodes::with_capacity(16);
    let clear = Color::TRANSPARENT;
    let blue = Color::rgb(70, 101, 225);
    let mut add = |parent, layout, color, radius| add(&mut tree, parent, layout, color, radius);
    let root = add(None, style([480, 300], true, 24, 0), clear, 0.0)?;
    let card = add(
        Some(root),
        style([432, 252], true, 24, 20),
        Color::WHITE,
        16.0,
    )?;
    let header = add(Some(card), style([384, 40], false, 0, 16), clear, 0.0)?;
    let icon = add(Some(header), style([40, 40], false, 0, 0), blue, 12.0)?;
    let bars = add(Some(header), style([240, 40], true, 4, 9), clear, 0.0)?;
    add(
        Some(bars),
        style([142, 10], false, 0, 0),
        Color(0x434957ff),
        5.0,
    )?;
    add(
        Some(bars),
        style([98, 7], false, 0, 0),
        Color(0xc2c8d3ff),
        3.5,
    )?;
    let field = add(
        Some(card),
        style([384, 46], false, 18, 0),
        Color(0xf2f4f8ff),
        9.0,
    )?;
    add(
        Some(field),
        style([104, 8], false, 0, 0),
        Color(0xb8bfcdff),
        4.0,
    )?;
    let row = add(Some(card), style([384, 40], false, 0, 12), clear, 0.0)?;
    let button = add(Some(row), style([116, 40], false, 0, 0), blue, 9.0)?;
    add(
        Some(row),
        style([94, 40], false, 0, 0),
        Color(0xe8ecf3ff),
        9.0,
    )?;
    tree.get_mut(icon).unwrap().context.mark = true;
    let available = Size {
        width: AvailableSpace::Definite(WIDTH as f32),
        height: AvailableSpace::Definite(HEIGHT as f32),
    };
    compute(&mut tree, root, available, |_, _, _, _| Size::ZERO)?;
    let mut pixels = vec![0; WIDTH as usize * HEIGHT as usize * 4];
    let mut surface = Surface::new(&mut pixels, WIDTH, HEIGHT)?;
    let mut renderer = Renderer::new(WIDTH as usize * HEIGHT as usize * 2);
    let mut stack = Vec::with_capacity(tree.len());
    let (first, _) = paint(&mut tree, root, &mut renderer, &mut surface, &mut stack)?;
    tree.update(button, Dirty::PAINT, |node| {
        node.context.color = Color::rgb(53, 80, 195)
    })?;
    compute(&mut tree, root, available, |_, _, _, _| Size::ZERO)?;
    let (changed, scene_bytes) = paint(&mut tree, root, &mut renderer, &mut surface, &mut stack)?;
    let start = Instant::now();
    for _ in 0..100 {
        paint(&mut tree, root, &mut renderer, &mut surface, &mut stack)?;
    }
    println!("100 warm offscreen redraws: {:?}", start.elapsed());
    println!("rebuilt scenes: initial {first}, paint-only change {changed}");
    println!(
        "reserved scene commands: {scene_bytes} bytes; mask storage: {} bytes",
        renderer.allocated_mask_bytes()
    );
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/aegle-software.png".into());
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(&output)?), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // The opaque frame background makes premultiplied and straight RGBA identical.
    encoder.write_header()?.write_image_data(surface.data())?;
    println!("saved {output}");
    Ok(())
}
