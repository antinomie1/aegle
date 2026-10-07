//! Geometry and CJK text rendered offscreen through wgpu and written as PPM.
use std::{fs::File, io::Write};

use aegle_render_wgpu::{Options, Renderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_text::{Blob, TextStyle, TextSystem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "/tmp/aegle-wgpu.ppm".into());
    let mut fonts = TextSystem::new();
    // The OFL subset is an example/test fixture, never embedded by the library.
    fonts.register_fonts(Blob::from(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec(),
    ))?;
    let paragraph = fonts.paragraph(
        "wgpu 中文 / 日本語 / 한글",
        &TextStyle {
            families: "Aegle Test CJK",
            size: 24.0,
            color: Color::rgb(32, 49, 75),
            ..TextStyle::default()
        },
    )?;
    let mut builder = SceneBuilder::new();
    let card = RoundedRect::new(Rect::new(16.0, 16.0, 368.0, 168.0), 16.0)?;
    builder.fill(card, Color::WHITE)?;
    builder.stroke(card, Color::rgb(215, 222, 232), 1.0)?;
    builder.push_transform(Affine::translation(36.0, 48.0)?)?;
    paragraph.paint(&mut builder)?;
    builder.pop()?;
    let scene = builder.finish()?;

    let mut renderer = Renderer::new(Options::default())?;
    println!("device: {}", renderer.device_name());
    let mut frame = renderer.begin_frame(400, 200, Color::rgb(238, 241, 246))?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut rgba = vec![0; 400 * 200 * 4];
    renderer.read_pixels(&mut rgba)?;

    // Opaque background: premultiplied bytes are also straight RGB.
    let mut file = File::create(&output)?;
    write!(file, "P6\n400 200\n255\n")?;
    for pixel in rgba.as_chunks::<4>().0 {
        file.write_all(&pixel[..3])?;
    }
    println!("wrote {}", output.to_string_lossy());
    Ok(())
}
