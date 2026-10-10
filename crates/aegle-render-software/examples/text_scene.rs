//! Retained paragraphs measured by Taffy and rendered without a window or GPU.
use aegle_core::{NodeId, Tree};
use aegle_layout::{
    AvailableSpace, Dimension, Edges, FlexDirection, LayoutNode, Size, Style, compute,
};
use aegle_render_software::{Renderer, Surface};
use aegle_scene::{Affine, Color, Point, Rect, RoundedRect, Scene, SceneBuilder};
use aegle_text::{Alignment, Blob, LineHeight, Paragraph, TextStyle, TextSystem};
use std::{error::Error, fs::File, io::BufWriter, sync::Arc, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Nodes = Tree<LayoutNode<Option<Paragraph>>>;
const WIDTH: u32 = 560;
const HEIGHT: u32 = 400;
const BACKGROUND: Color = Color::rgb(237, 240, 244);

fn add_text(
    tree: &mut Nodes,
    parent: NodeId,
    fonts: &mut TextSystem,
    text: &str,
    size: f32,
    color: Color,
) -> Result<()> {
    let paragraph = fonts.paragraph(
        text,
        &TextStyle {
            families: "Aegle Test CJK",
            size,
            color,
            line_height: LineHeight::FontSizeRelative(1.5),
            ..TextStyle::default()
        },
    )?;
    assert_eq!(paragraph.diagnostics(), Default::default());
    tree.insert(
        Some(parent),
        LayoutNode::with_style(
            Style {
                flex_shrink: 0.0,
                ..Style::default()
            },
            Some(paragraph),
        ),
    )?;
    Ok(())
}

fn layout(tree: &mut Nodes, root: NodeId) -> Result<usize> {
    let mut measurements = 0;
    let mut error = None;
    compute(
        tree,
        root,
        Size {
            width: AvailableSpace::Definite(WIDTH as f32),
            height: AvailableSpace::Definite(HEIGHT as f32),
        },
        |_, text, known, available| {
            let Some(paragraph) = text else {
                return Size::ZERO;
            };
            measurements += 1;
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                AvailableSpace::MinContent => Some(paragraph.content_widths().min),
                AvailableSpace::MaxContent => None,
            });
            match paragraph.reflow(width, Alignment::Start) {
                Ok(size) => Size {
                    width: known.width.unwrap_or(size.width),
                    height: known.height.unwrap_or(size.height),
                },
                Err(failure) => {
                    error = Some(failure);
                    Size::ZERO
                }
            }
        },
    )?;
    if let Some(error) = error {
        return Err(error.into());
    }
    Ok(measurements)
}

fn record(tree: &mut Nodes, root: NodeId) -> Result<Scene> {
    let mut builder = SceneBuilder::new();
    let card = RoundedRect::new(Rect::new(24.0, 24.0, 512.0, 352.0), 18.0);
    builder.fill(card, Color::WHITE);
    builder.stroke(card, Color::rgb(215, 222, 232), 1.0);
    builder.push_clip(card);
    let mut pending = vec![(root, Point::default())];
    while let Some((id, parent_origin)) = pending.pop() {
        let node = tree.get_mut(id).unwrap();
        let bounds = node.bounds();
        let origin = Point::new(
            parent_origin.x + bounds.origin.x,
            parent_origin.y + bounds.origin.y,
        );
        if let Some(paragraph) = &mut node.context {
            // Taffy may last measure with an intrinsic width. Paint at its final width.
            paragraph.reflow(Some(bounds.size.width), Alignment::Start)?;
            builder.push_transform(Affine::translation(origin.x, origin.y));
            builder.push_clip(RoundedRect::new(
                Rect::new(0.0, 0.0, bounds.size.width, bounds.size.height),
                0.0,
            ));
            paragraph.paint(&mut builder);
            builder.pop().pop();
        }
        pending.extend(tree.children(id)?.rev().map(|child| (child, origin)));
    }
    builder.pop();
    Ok(builder.finish())
}

fn main() -> Result<()> {
    let mut fonts = TextSystem::new();
    // This tiny OFL fixture belongs only to examples/tests, not the library.
    let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> =
        Arc::new(include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice());
    fonts.register_fonts(Blob::new(bytes))?;
    let mut tree = Nodes::with_capacity(6);
    let root = tree.insert(
        None,
        LayoutNode::with_style(
            Style {
                size: Size {
                    width: Dimension::length(WIDTH as f32),
                    height: Dimension::length(HEIGHT as f32),
                },
                flex_direction: FlexDirection::Column,
                padding: Edges::length(48),
                gap: Size::length(10),
                ..Style::default()
            },
            None,
        ),
    )?;
    for (text, size, color) in [
        ("AEGLE / RETAINED TEXT", 11.0, Color::rgb(64, 96, 182)),
        ("Hello, world.", 32.0, Color::rgb(34, 43, 61)),
        ("你好世界中文 / 日本語 / 한글", 24.0, Color::rgb(50, 66, 93)),
        (
            "Aegle keeps text shaped while Taffy sets the width. Resize to wrap; \
             reuse the scene to paint. CJK glyphs are rasterized only when needed.",
            15.0,
            Color::rgb(87, 99, 118),
        ),
        (
            "Software pixels. Shared font bytes. No GPU required.",
            11.0,
            Color::rgb(93, 113, 103),
        ),
    ] {
        add_text(&mut tree, root, &mut fonts, text, size, color)?;
    }
    let first = layout(&mut tree, root)?;
    let warm = layout(&mut tree, root)?;
    let scene = record(&mut tree, root)?;
    let mut pixels = vec![0; WIDTH as usize * HEIGHT as usize * 4];
    let mut surface = Surface::new(&mut pixels, WIDTH, HEIGHT)?;
    let mut renderer = Renderer::new(WIDTH as usize * HEIGHT as usize * 3);
    renderer
        .begin_frame(&mut surface, BACKGROUND)
        .draw(&scene, Affine::IDENTITY)?;
    let initial_pixels = surface.data().to_vec();
    let initial_cache = renderer.glyph_cache().stats();
    let start = Instant::now();
    for _ in 0..10 {
        renderer
            .begin_frame(&mut surface, BACKGROUND)
            .draw(&scene, Affine::IDENTITY)?;
    }
    assert_eq!(surface.data(), initial_pixels);
    assert_eq!(renderer.glyph_cache().stats(), initial_cache);
    println!("Taffy measurements: initial {first}, unchanged {warm}");
    println!(
        "10 warm offscreen redraws: {:?}; pixels unchanged",
        start.elapsed()
    );
    println!("glyph cache: {initial_cache:?}");
    println!("retained scene storage: {} bytes", scene.allocated_bytes());
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/aegle-text.png".into());
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(&output)?), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // Opaque background makes premultiplied and straight output bytes identical.
    encoder.write_header()?.write_image_data(surface.data())?;
    println!("saved {output}");
    Ok(())
}
