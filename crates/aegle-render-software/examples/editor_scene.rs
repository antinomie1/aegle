//! Headless retained editor: selection, transient preedit, commit and delta undo.
use aegle_render_software::{Renderer, Surface};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_text::{Blob, Editor, EditorOptions, EditorPaint, Selection, TextStyle, TextSystem};
use std::{error::Error, fs::File, io::BufWriter, sync::Arc, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const WIDTH: u32 = 480;
const HEIGHT: u32 = 276;

fn row(
    builder: &mut SceneBuilder,
    fonts: &mut TextSystem,
    editor: &Editor,
    title: &str,
    y: f32,
) -> Result<()> {
    let label = fonts.paragraph(
        title,
        &TextStyle {
            families: "Aegle Test CJK",
            size: 11.0,
            color: Color::rgb(88, 103, 130),
            ..Default::default()
        },
    )?;
    builder.push_transform(Affine::translation(28.0, y));
    label.paint(builder);
    let field = RoundedRect::new(Rect::new(0.0, 28.0, 424.0, 56.0), 10.0);
    builder.fill(field, Color::WHITE);
    builder.stroke(field, Color::rgb(188, 201, 225), 1.0);
    builder.push_clip(field);
    builder.push_transform(Affine::translation(16.0, 36.0));
    editor.paint(
        builder,
        EditorPaint {
            caret: Some(Color::rgb(50, 91, 166)),
            preedit: Some(Color::rgb(50, 91, 166)),
            ..Default::default()
        },
    );
    builder.pop().pop().pop();
    Ok(())
}

fn main() -> Result<()> {
    let mut fonts = TextSystem::new();
    fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    let mut editor = fonts.editor(
        "Hello, 世界",
        &TextStyle {
            families: "Aegle Test CJK",
            size: 24.0,
            color: Color::rgb(35, 48, 71),
            ..Default::default()
        },
        EditorOptions::default(),
    )?;
    fonts.edit(&mut editor).select(Selection {
        anchor: 7,
        focus: 13,
    })?;
    editor.take_changes();
    let start = Instant::now();
    for index in 0..100 {
        fonts
            .edit(&mut editor)
            .set_preedit(if index % 2 == 0 { "你好" } else { "中文" }, None)?;
    }
    println!("100 retained preedit updates: {:?}", start.elapsed());
    assert_eq!(editor.history_stats().bytes, 0);
    fonts.edit(&mut editor).set_preedit(
        "你好",
        Some(Selection {
            anchor: 3,
            focus: 6,
        }),
    )?;
    assert_eq!(editor.text(), "Hello, 世界");
    assert!(!editor.take_changes().value);
    let mut builder = SceneBuilder::new();
    row(
        &mut builder,
        &mut fonts,
        &editor,
        "IME PREEDIT / committed value unchanged",
        24.0,
    )?;
    fonts.edit(&mut editor).commit("你好")?;
    assert_eq!(editor.text(), "Hello, 你好");
    assert!(editor.take_changes().value);
    row(
        &mut builder,
        &mut fonts,
        &editor,
        "COMMITTED / one undo step",
        148.0,
    )?;
    let scene = builder.finish();
    fonts.edit(&mut editor).undo()?;
    assert_eq!(editor.text(), "Hello, 世界");
    fonts.edit(&mut editor).redo()?;
    assert_eq!(editor.text(), "Hello, 你好");
    println!(
        "composition, commit, undo and redo preserved; history: {:?}",
        editor.history_stats()
    );
    let mut pixels = vec![0; WIDTH as usize * HEIGHT as usize * 4];
    let mut surface = Surface::new(&mut pixels, WIDTH, HEIGHT)?;
    let mut renderer = Renderer::default();
    renderer
        .begin_frame(&mut surface, Color::rgb(239, 242, 247))
        .draw(&scene, Affine::IDENTITY)?;
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/aegle-editor.png".into());
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(&output)?), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(surface.data())?;
    println!(
        "saved {output}; glyph cache: {:?}",
        renderer.glyph_cache().stats()
    );
    Ok(())
}
