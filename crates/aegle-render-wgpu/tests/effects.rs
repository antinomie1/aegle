//! Gradient fills and shadows, transformed and clipped, match the software
//! renderer, which evaluates the same formulas at the same pixel centers.

use aegle_render_wgpu::{Options, Renderer};
use aegle_scene::{Affine, Color, Gradient, GradientStop, Point, Rect, RoundedRect, SceneBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: u32 = 96;

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn gradients_and_shadows_match_software() -> Result {
    let stop = |offset, color| GradientStop { offset, color };
    let stops = [
        stop(0.0, Color::rgb(255, 0, 0)),
        stop(0.5, Color::rgba(0, 160, 0, 96)),
        stop(0.5, Color::rgb(20, 40, 220)),
        stop(1.0, Color::rgba(250, 200, 0, 200)),
    ];
    let shape = |x, y, w, h, r| RoundedRect::new(Rect::new(x, y, w, h), r);
    let linear = Gradient::linear(Point::new(4.0, 4.0), Point::new(60.0, 30.0), &stops)?;
    let radial = Gradient::radial(Point::new(20.0, 20.0), 18.0, &stops[1..])?;
    let mut builder = SceneBuilder::new();
    builder
        .shadow(
            shape(10.0, 12.0, 40.0, 24.0, 8.0),
            Color::rgba(0, 0, 80, 160),
            5.0,
        )
        .fill_gradient(shape(4.0, 4.0, 56.0, 30.0, 6.0), &linear)
        .push_clip(shape(40.0, 40.0, 52.0, 52.0, 12.0))
        .push_transform(Affine::new([0.8, 0.6, -0.6, 0.8, 60.0, 34.0])?)
        .fill_gradient(shape(0.0, 0.0, 40.0, 40.0, 0.0), &radial)
        .shadow(shape(4.0, 44.0, 30.0, 12.0, 0.0), Color::BLACK, 2.5)
        .pop()
        .pop();
    let scene = builder.finish();
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::WHITE)?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_tiny_skia::Surface::new(&mut expected, SIZE, SIZE)?;
    aegle_render_tiny_skia::Renderer::default()
        .begin_frame(&mut surface, Color::WHITE)
        .draw(&scene, Affine::IDENTITY)?;
    let differences = pixels.iter().zip(&expected).map(|(a, b)| a.abs_diff(*b));
    let (total, worst) = differences.fold((0u64, 0u8), |(sum, max), d| {
        (sum + u64::from(d), max.max(d))
    });
    let mean = total as f64 / pixels.len() as f64;
    eprintln!("GPU/software channel difference: mean {mean:.3}, worst {worst}");
    // Only antialiased edges differ by more than rounding.
    assert!(mean < 0.5);
    let interior = [(30, 20), (14, 40), (70, 60)];
    for (x, y) in interior {
        let i = (y * SIZE as usize + x) * 4;
        for c in 0..4 {
            assert!(pixels[i + c].abs_diff(expected[i + c]) <= 2, "({x}, {y})");
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn cuts_per_corner_radii_and_inset_shadows_match_software() -> Result {
    let leaf = |x, y, w, h| RoundedRect::with_radii(Rect::new(x, y, w, h), [16.0, 0.0, 8.0, 0.0]);
    let card = RoundedRect::new(Rect::new(52.0, 8.0, 36.0, 36.0), 8.0);
    let hole = RoundedRect::new(Rect::new(56.0, 12.0, 32.0, 32.0), 4.0);
    let mut builder = SceneBuilder::new();
    builder
        .fill(leaf(4.0, 4.0, 40.0, 40.0), Color::rgb(30, 90, 220))
        .fill(card, Color::WHITE)
        .inset_shadow(card, hole, Color::rgba(0, 0, 0, 160), 3.0)
        .fill(
            RoundedRect::new(Rect::new(8.0, 52.0, 80.0, 40.0), 0.0),
            Color::rgb(200, 30, 30),
        )
        .cut(RoundedRect::new(Rect::new(30.0, 56.0, 32.0, 32.0), 16.0));
    let scene = builder.finish();
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::TRANSPARENT)?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_tiny_skia::Surface::new(&mut expected, SIZE, SIZE)?;
    aegle_render_tiny_skia::Renderer::default()
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&scene, Affine::IDENTITY)?;
    let total: u64 = (pixels.iter().zip(&expected))
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    let mean = total as f64 / pixels.len() as f64;
    eprintln!("GPU/software difference: mean {mean:.3}");
    assert!(mean < 0.5);
    let alpha = |x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4 + 3];
    assert_eq!(
        (alpha(5, 5), alpha(42, 5)),
        (0, 255),
        "rounded and square corners"
    );
    assert_eq!((alpha(46, 72), alpha(12, 72)), (0, 255), "the cut circle");
    Ok(())
}
