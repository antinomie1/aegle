//! Gradient fills and shadows, transformed and clipped, match the software
//! renderer, which evaluates the same formulas at the same pixel centers.

use aegle_render_vulkan::{Options, Renderer};
use aegle_scene::{Affine, Color, Gradient, GradientStop, Point, Rect, RoundedRect, SceneBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: u32 = 96;

#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
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
            shape(10.0, 12.0, 40.0, 24.0, 8.0)?,
            Color::rgba(0, 0, 80, 160),
            5.0,
        )?
        .fill_gradient(shape(4.0, 4.0, 56.0, 30.0, 6.0)?, &linear)?
        .push_clip(shape(40.0, 40.0, 52.0, 52.0, 12.0)?)?
        .push_transform(Affine::new([0.8, 0.6, -0.6, 0.8, 60.0, 34.0])?)?
        .fill_gradient(shape(0.0, 0.0, 40.0, 40.0, 0.0)?, &radial)?
        .shadow(shape(4.0, 44.0, 30.0, 12.0, 0.0)?, Color::BLACK, 2.5)?
        .pop()?
        .pop()?;
    let scene = builder.finish()?;
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::WHITE)?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_software::Surface::new(&mut expected, SIZE, SIZE)?;
    aegle_render_software::Renderer::default()
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
