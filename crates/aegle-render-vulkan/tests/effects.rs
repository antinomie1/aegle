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
    let (pixels, expected) = render(&scene)?;
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
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn per_corner_radii_match_software() -> Result {
    // Round top-left and bottom-right corners only, as fill, stroke, shadow
    // and clip.
    let leaf = |x, y, w, h| RoundedRect::with_radii(Rect::new(x, y, w, h), [16.0, 0.0, 8.0, 0.0]);
    let mut builder = SceneBuilder::new();
    builder
        .shadow(leaf(52.0, 8.0, 36.0, 36.0), Color::rgba(0, 0, 80, 160), 4.0)
        .fill(leaf(4.0, 4.0, 40.0, 40.0), Color::rgb(30, 90, 220))
        .stroke(leaf(52.0, 52.0, 36.0, 36.0), Color::rgb(200, 30, 30), 4.0)
        .push_clip(leaf(4.0, 52.0, 40.0, 40.0))
        .fill(
            RoundedRect::new(Rect::new(0.0, 48.0, 48.0, 48.0), 0.0),
            Color::BLACK,
        )
        .pop();
    let (pixels, expected) = render(&builder.finish())?;
    let differences = pixels.iter().zip(&expected).map(|(a, b)| a.abs_diff(*b));
    let total: u64 = differences.map(u64::from).sum();
    let mean = total as f64 / pixels.len() as f64;
    eprintln!("GPU/software per-corner difference: mean {mean:.3}");
    assert!(mean < 0.5);
    let at = |pixels: &[u8], x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4];
    for pixels in [&pixels, &expected] {
        // The fill's rounded top-left corner leaves white; its square
        // top-right corner is blue. The clipped black square follows the same.
        assert_eq!(at(pixels, 5, 5), 255);
        assert!(at(pixels, 42, 5) < 60);
        assert_eq!(at(pixels, 5, 53), 255);
        assert!(at(pixels, 42, 53) < 10);
    }
    Ok(())
}

#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn inset_shadows_match_software() -> Result {
    let rect = |x, y, w, h| Rect::new(x, y, w, h);
    // A rounded card pressed in from the top left, and a rotated square
    // whose hole spreads to nothing, so it is fully shaded.
    let card = RoundedRect::new(rect(8.0, 8.0, 48.0, 32.0), 10.0);
    let hole = RoundedRect::new(rect(12.0, 12.0, 44.0, 28.0), 6.0);
    let mut builder = SceneBuilder::new();
    builder
        .fill(card, Color::rgb(240, 240, 250))
        .inset_shadow(card, hole, Color::rgba(0, 0, 0, 160), 3.0)
        .push_transform(Affine::new([0.8, 0.6, -0.6, 0.8, 60.0, 50.0])?)
        .inset_shadow(
            RoundedRect::new(rect(0.0, 0.0, 24.0, 24.0), 0.0),
            RoundedRect::new(rect(12.0, 12.0, 0.0, 0.0), 0.0),
            Color::rgb(200, 0, 0),
            2.0,
        )
        .pop();
    let (pixels, expected) = render(&builder.finish())?;
    let total: u64 = (pixels.iter().zip(&expected))
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    let mean = total as f64 / pixels.len() as f64;
    eprintln!("GPU/software inset shadow difference: mean {mean:.3}");
    assert!(mean < 0.5);
    let at = |pixels: &[u8], x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4];
    for pixels in [&pixels, &expected] {
        // Dark along the pressed-in top left, light in the middle and toward
        // the bottom right, white outside the card.
        let (edge, middle) = (at(pixels, 20, 9), at(pixels, 40, 30));
        assert!(edge + 60 < middle && middle > 230, "{edge} {middle}");
        assert_eq!(at(pixels, 4, 4), 255);
    }
    Ok(())
}

/// The scene drawn by the GPU and by the software renderer, as RGBA rows.
fn render(scene: &aegle_scene::Scene) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::WHITE)?;
    frame.draw(scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_tiny_skia::Surface::new(&mut expected, SIZE, SIZE)?;
    aegle_render_tiny_skia::Renderer::default()
        .begin_frame(&mut surface, Color::WHITE)
        .draw(scene, Affine::IDENTITY)?;
    Ok((pixels, expected))
}
