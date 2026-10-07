//! Shared CJK shaping and on-demand glyphs rendered to an offscreen Vulkan image.
use std::{
    fs::File,
    io::{BufWriter, Write},
};

use aegle_render_vulkan::{Options, Renderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_text::{Blob, TextStyle, TextSystem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "/tmp/aegle-vulkan-text.ppm".into());
    let mut fonts = TextSystem::new();
    // The OFL subset is an example/test fixture, never embedded by the library.
    fonts.register_fonts(Blob::from(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec(),
    ))?;
    let mut builder = SceneBuilder::new();
    let card = RoundedRect::new(Rect::new(24.0, 24.0, 752.0, 432.0), 24.0)?;
    builder.fill(card, Color::WHITE)?;
    builder.stroke(card, Color::rgb(215, 222, 232), 1.0)?;
    for (text, size, color, transform, clip) in [
        (
            "AEGLE / VULKAN TEXT",
            26.0,
            Color::rgb(32, 49, 75),
            Affine::translation(48.0, 44.0)?,
            None,
        ),
        (
            "你好世界中文 / 日本語 / 한글",
            28.0,
            Color::rgb(45, 94, 155),
            Affine::translation(48.0, 104.0)?,
            None,
        ),
        (
            "Quarter-pixel origin: 0.25 / 0.50",
            17.0,
            Color::rgb(78, 95, 115),
            Affine::translation(48.25, 168.5)?,
            None,
        ),
        (
            "Clipped text / 你好世界中文 / 日本語 / 한글",
            25.0,
            Color::rgba(32, 113, 94, 210),
            Affine::translation(48.0, 224.0)?,
            Some(Rect::new(48.0, 232.0, 308.0, 28.0)),
        ),
        (
            "Retained geometry + 中文",
            25.0,
            Color::rgba(114, 72, 148, 200),
            Affine::new([0.966, -0.259, 0.259, 0.966, 388.0, 312.0])?,
            Some(Rect::new(384.0, 248.0, 352.0, 136.0)),
        ),
        (
            "Shared fonts. Warm atlas. Explicit readback.",
            16.0,
            Color::rgb(97, 110, 127),
            Affine::translation(48.0, 396.0)?,
            None,
        ),
    ] {
        if let Some(rect) = clip {
            builder.push_clip(RoundedRect::new(rect, 0.0)?)?;
        }
        builder.push_transform(transform)?;
        let paragraph = fonts.paragraph(
            text,
            &TextStyle {
                families: "Aegle Test CJK",
                size,
                color,
                ..TextStyle::default()
            },
        )?;
        assert_eq!(paragraph.diagnostics(), Default::default());
        paragraph.paint(&mut builder)?;
        builder.pop()?;
        if clip.is_some() {
            builder.pop()?;
        }
    }
    builder.fill(
        RoundedRect::new(Rect::new(48.0, 316.0, 272.0, 40.0), 12.0)?,
        Color::rgb(221, 235, 248),
    )?;
    let scene = builder.finish()?;
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(800, 480, Color::rgb(240, 243, 247))?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; 800 * 480 * 4];
    renderer.read_pixels(&mut pixels)?;
    let mut file = BufWriter::new(File::create(&output)?);
    file.write_all(b"P6\n800 480\n255\n")?;
    for pixel in pixels.as_chunks::<4>().0 {
        file.write_all(&pixel[..3])?;
    }
    file.flush()?;
    println!("{} -> {}", renderer.device_name(), output.to_string_lossy());
    println!("text resources: {:?}", renderer.text_stats());
    Ok(())
}
