//! Nested layers, group opacity, clips and backdrop blur match the software
//! renderer, which uses the same box passes and compositing rules.

use aegle_render_vulkan::{Options, Renderer};
use aegle_scene::{
    Affine, Color, Gradient, GradientStop, Layer, Point, Rect, RoundedRect, Scene, SceneBuilder,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: u32 = 96;

enum Op {
    Draw(Scene),
    Push(Layer),
    Pop,
}

fn ops() -> Result<Vec<Op>> {
    let shape = |x, y, w, h, r| RoundedRect::new(Rect::new(x, y, w, h), r);
    let stops = [
        GradientStop {
            offset: 0.0,
            color: Color::rgb(230, 40, 40),
        },
        GradientStop {
            offset: 1.0,
            color: Color::rgb(30, 60, 220),
        },
    ];
    let gradient = Gradient::linear(Point::new(0.0, 0.0), Point::new(96.0, 0.0), &stops)?;
    let mut backdrop = SceneBuilder::new();
    backdrop.fill_gradient(shape(0.0, 0.0, 96.0, 96.0, 0.0)?, &gradient)?;
    for i in 0..6 {
        let x = 8.0 + i as f32 * 14.0;
        backdrop.fill(shape(x, 8.0, 6.0, 80.0, 0.0)?, Color::WHITE)?;
    }
    let mut content = SceneBuilder::new();
    content
        .fill(shape(20.0, 20.0, 40.0, 40.0, 6.0)?, Color::rgb(0, 150, 60))?
        .fill(shape(40.0, 40.0, 40.0, 40.0, 6.0)?, Color::rgb(0, 150, 60))?;
    let all = Rect::new(0.0, 0.0, 96.0, 96.0);
    let frosted = Layer::new(
        shape(12.0, 12.0, 72.0, 72.0, 10.0)?,
        Affine::IDENTITY,
        all,
        Some(Rect::new(0.0, 0.0, 90.0, 96.0)),
        0.9,
        3.0,
    )?;
    let faded = Layer::new(
        shape(0.0, 0.0, 96.0, 96.0, 0.0)?,
        Affine::IDENTITY,
        Rect::new(16.0, 16.0, 70.0, 70.0),
        None,
        0.5,
        0.0,
    )?;
    Ok(vec![
        Op::Draw(backdrop.finish()?),
        Op::Push(frosted),
        Op::Push(faded),
        Op::Draw(content.finish()?),
        Op::Pop,
        Op::Pop,
    ])
}

#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn layers_and_backdrop_blur_match_software() -> Result {
    let ops = ops()?;
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::WHITE)?;
    for op in &ops {
        match op {
            Op::Draw(scene) => frame.draw(scene, Affine::IDENTITY)?,
            Op::Push(layer) => frame.push_layer(layer)?,
            Op::Pop => frame.pop_layer()?,
        }
    }
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_software::Surface::new(&mut expected, SIZE, SIZE)?;
    let mut software = aegle_render_software::Renderer::default();
    {
        let mut frame = software.begin_frame(&mut surface, Color::WHITE);
        for op in &ops {
            match op {
                Op::Draw(scene) => frame.draw(scene, Affine::IDENTITY)?,
                Op::Push(layer) => frame.push_layer(layer)?,
                Op::Pop => frame.pop_layer()?,
            }
        }
    }
    let differences = pixels.iter().zip(&expected).map(|(a, b)| a.abs_diff(*b));
    let (total, worst) = differences.fold((0u64, 0u8), |(sum, max), d| {
        (sum + u64::from(d), max.max(d))
    });
    let mean = total as f64 / pixels.len() as f64;
    eprintln!("GPU/software channel difference: mean {mean:.3}, worst {worst}");
    // Only antialiased shape edges differ by more than rounding.
    assert!(mean < 0.5);
    // Inside the blur, the overlap of the faded squares, and outside the clip.
    for (x, y) in [(30, 30), (50, 50), (70, 24), (93, 50)] {
        let i = (y * SIZE as usize + x) * 4;
        for c in 0..4 {
            let (gpu, cpu) = (pixels[i + c], expected[i + c]);
            assert!(
                gpu.abs_diff(cpu) <= 3,
                "({x}, {y}) channel {c}: GPU {gpu}, software {cpu}"
            );
        }
    }
    Ok(())
}
