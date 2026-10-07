//! Offscreen Vulkan geometry; writes an opaque PPM with no image-codec dependency.
use std::{
    fs::File,
    io::{BufWriter, Write},
};

use aegle_render_vulkan::{Options, Renderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};

fn shape(x: f32, y: f32, w: f32, h: f32, radius: f32) -> RoundedRect {
    RoundedRect::new(Rect::new(x, y, w, h), radius).unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "aegle-vulkan.ppm".into());
    let mut renderer = Renderer::new(Options::default())?;
    let mut builder = SceneBuilder::new();
    builder.fill(shape(32.0, 32.0, 460.0, 416.0, 28.0), Color::WHITE)?;
    builder.stroke(
        shape(32.0, 32.0, 460.0, 416.0, 28.0),
        Color::rgb(214, 221, 230),
        2.0,
    )?;
    builder.push_clip(shape(56.0, 56.0, 412.0, 248.0, 20.0))?;
    builder.fill(
        shape(56.0, 56.0, 412.0, 248.0, 0.0),
        Color::rgb(225, 237, 247),
    )?;
    builder.push_transform(Affine::new([0.866, 0.5, -0.5, 0.866, 180.0, 32.0])?)?;
    builder.fill(
        shape(0.0, 0.0, 220.0, 220.0, 32.0),
        Color::rgba(49, 103, 180, 210),
    )?;
    builder.stroke(
        shape(44.0, 44.0, 180.0, 180.0, 28.0),
        Color::rgba(255, 255, 255, 190),
        12.0,
    )?;
    builder.pop()?.pop()?;
    for (y, width, color) in [
        (336.0, 280.0, Color::rgb(56, 68, 88)),
        (366.0, 356.0, Color::rgb(181, 191, 204)),
    ] {
        builder.fill(shape(64.0, y, width, 12.0, 6.0), color)?;
    }
    builder.fill(
        shape(524.0, 32.0, 244.0, 196.0, 28.0),
        Color::rgb(29, 47, 65),
    )?;
    builder.fill(
        shape(552.0, 64.0, 64.0, 64.0, 32.0),
        Color::rgb(150, 213, 185),
    )?;
    builder.fill(
        shape(552.0, 156.0, 180.0, 12.0, 6.0),
        Color::rgba(255, 255, 255, 170),
    )?;
    builder.fill(shape(524.0, 252.0, 244.0, 196.0, 28.0), Color::WHITE)?;
    for (x, y, h) in [
        (552.0, 340.0, 76.0),
        (598.0, 308.0, 108.0),
        (644.0, 328.0, 88.0),
        (690.0, 284.0, 132.0),
    ] {
        builder.fill(shape(x, y, 28.0, h, 10.0), Color::rgb(76, 133, 190))?;
    }
    let scene = builder.finish()?;
    let mut frame = renderer.begin_frame(800, 480, Color::rgb(240, 243, 247))?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; 800 * 480 * 4];
    renderer.read_pixels(&mut pixels)?;
    let mut output = BufWriter::new(File::create(&path)?);
    output.write_all(b"P6\n800 480\n255\n")?;
    for pixel in pixels.as_chunks::<4>().0 {
        output.write_all(&pixel[..3])?;
    }
    output.flush()?;
    let stats = renderer.stats();
    println!(
        "{} -> {} (device={} B, recording={} B)",
        renderer.device_name(),
        path.to_string_lossy(),
        stats.device_bytes,
        stats.recording_bytes
    );
    Ok(())
}
