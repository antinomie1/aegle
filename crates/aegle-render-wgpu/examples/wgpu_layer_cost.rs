//! Times a 1280×800 frame of stripes alone, with a 480×320 group-opacity
//! layer, and with a σ=8 backdrop blur under that layer.
//!
//! `cargo run --release -p aegle-render-wgpu --features text --example wgpu_layer_cost`
//!
//! Each time covers recording, submission and the GPU work up to a readback
//! of the frame, which costs the same in every case.
use std::time::Instant;

use aegle_render_wgpu::{Options, Renderer};
use aegle_scene::{Affine, Color, Layer, Rect, RoundedRect, Scene, SceneBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: (u32, u32) = (1280, 800);

fn main() -> Result {
    let (stripes, panel) = scenes()?;
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    let mut renderer = Renderer::new(Options::default())?;
    println!("device: {}", renderer.device_name());
    for (name, layer) in [("plain", None), ("opacity", Some(0.0)), ("blur", Some(8.0))] {
        let mut frame_time = |pixels: &mut [u8]| -> Result<f64> {
            let start = Instant::now();
            let mut frame = renderer.begin_frame(SIZE.0, SIZE.1, Color::WHITE)?;
            frame.draw(&stripes, Affine::IDENTITY)?;
            if let Some(blur) = layer {
                frame.push_layer(&self::layer(blur)?)?;
                frame.draw(&panel, Affine::IDENTITY)?;
                frame.pop_layer()?;
            }
            frame.finish()?;
            renderer.read_pixels(pixels)?;
            Ok(start.elapsed().as_secs_f64() * 1e3)
        };
        frame_time(&mut pixels)?;
        let mut times: Vec<_> = (0..50)
            .map(|_| frame_time(&mut pixels))
            .collect::<Result<_>>()?;
        times.sort_by(f64::total_cmp);
        println!("{name}: median {:.3} ms", times[times.len() / 2]);
    }
    Ok(())
}

/// Blue stripes over the frame, and a translucent card for the layer.
fn scenes() -> Result<(Scene, Scene)> {
    let mut builder = SceneBuilder::new();
    for i in 0..SIZE.0 / 16 {
        let stripe = Rect::new(i as f32 * 16.0, 0.0, 8.0, SIZE.1 as f32);
        builder.fill(RoundedRect::new(stripe, 0.0), Color::rgb(40, 80, 220));
    }
    let stripes = builder.finish();
    let mut builder = SceneBuilder::new();
    let card = RoundedRect::new(Rect::new(400.0, 240.0, 480.0, 320.0), 16.0);
    builder.fill(card, Color::rgba(255, 255, 255, 140));
    Ok((stripes, builder.finish()))
}

/// The card's layer, its content range the card itself as `Ui` gives it.
fn layer(blur: f32) -> Result<Layer> {
    let bounds = Rect::new(400.0, 240.0, 480.0, 320.0);
    let shape = RoundedRect::new(bounds, 16.0);
    Ok(Layer::new(
        shape,
        Affine::IDENTITY,
        bounds,
        None,
        0.8,
        blur,
    )?)
}
