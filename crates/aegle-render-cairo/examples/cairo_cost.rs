//! Per-draw cost of the two CPU backends on one representative UI scene.
//!
//! Run with `cargo run -p aegle-render-cairo --example cairo_cost --release`.
//! Reports wall time for a full frame of pixel-aligned and antialiased shapes,
//! so the choice between the rasterizers is measured rather than assumed.
use aegle_render_cairo::{Renderer as CairoRenderer, Surface};
use aegle_render_tiny_skia::Renderer as SkiaRenderer;
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use std::time::Instant;

const SIZE: u32 = 1280;
const HEIGHT: u32 = 800;
const FRAMES: usize = 200;

fn scene(rounded: bool) -> aegle_scene::Scene {
    let mut builder = SceneBuilder::new();
    // A controls-like page: cards and borders, sharp or rounded.
    for row in 0..14 {
        for column in 0..8 {
            let x = 8.0 + column as f32 * 158.0;
            let y = 8.0 + row as f32 * 56.0;
            let radius = if rounded { 6.0 } else { 0.0 };
            builder.fill(
                RoundedRect::new(Rect::new(x, y, 150.0, 48.0), radius),
                Color::rgba(250, 250, 250, 255),
            );
            builder.stroke(
                RoundedRect::new(Rect::new(x + 8.0, y + 8.0, 134.0, 32.0), radius),
                Color::rgb(180, 180, 190),
                1.0,
            );
        }
    }
    builder.finish()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for rounded in [false, true] {
        let scene = scene(rounded);
        let mut pixels = vec![0u8; (SIZE * HEIGHT * 4) as usize];
        let skia = measure(SkiaRenderer::default(), &scene, &mut pixels)?;
        let cairo = measure(CairoRenderer::default(), &scene, &mut pixels)?;
        println!(
            "rounded={rounded}: tiny-skia {skia:.3} ms, cairo {cairo:.3} ms per frame \
             ({FRAMES} frames, {SIZE}x{HEIGHT}, {} shapes)",
            scene.len()
        );
    }
    Ok(())
}

fn measure<R: aegle_render_cpu::Rasterizer + Default>(
    mut renderer: aegle_render_cpu::Renderer<R>,
    scene: &aegle_scene::Scene,
    pixels: &mut [u8],
) -> Result<f64, Box<dyn std::error::Error>> {
    // One warm frame allocates masks and flattens the rasterizer's caches.
    {
        let mut surface = Surface::new(pixels, SIZE, HEIGHT)?;
        renderer
            .begin_frame(&mut surface, Color::WHITE)
            .draw(scene, Affine::IDENTITY)?;
    }
    let start = Instant::now();
    for _ in 0..FRAMES {
        let mut surface = Surface::new(pixels, SIZE, HEIGHT)?;
        renderer
            .begin_frame(&mut surface, Color::WHITE)
            .draw(scene, Affine::IDENTITY)?;
    }
    Ok(start.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64)
}
