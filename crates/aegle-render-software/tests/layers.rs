//! Layers composite their content once at an opacity; backdrop blur softens
//! what lies beneath inside the layer shape only; budgets and pairing hold.
use aegle_render_software::{RenderError, Renderer, Surface};
use aegle_scene::{Affine, Color, Layer, Rect, RoundedRect, Scene, SceneBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: usize = 64;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Result<RoundedRect> {
    Ok(RoundedRect::new(Rect::new(x, y, width, height), 0.0)?)
}

fn fills(shapes: &[(RoundedRect, Color)]) -> Result<Scene> {
    let mut builder = SceneBuilder::new();
    for &(shape, color) in shapes {
        builder.fill(shape, color)?;
    }
    Ok(builder.finish()?)
}

fn layer(shape: RoundedRect, opacity: f32, blur: f32) -> Result<Layer> {
    let all = Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32);
    Ok(Layer::new(
        shape,
        Affine::IDENTITY,
        all,
        None,
        opacity,
        blur,
    )?)
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    pixels[(y * SIZE + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn group_opacity_composites_overlapping_content_once() -> Result {
    let red = Color::rgb(220, 30, 30);
    let content = fills(&[
        (rect(8.0, 8.0, 32.0, 32.0)?, red),
        (rect(24.0, 24.0, 32.0, 32.0)?, red),
    ])?;
    let mut pixels = vec![0; SIZE * SIZE * 4];
    let mut surface = Surface::new(&mut pixels, SIZE as u32, SIZE as u32)?;
    let mut renderer = Renderer::default();
    {
        let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
        frame.push_layer(&layer(rect(0.0, 0.0, 64.0, 64.0)?, 0.5, 0.0)?)?;
        frame.draw(&content, Affine::IDENTITY)?;
        frame.pop_layer()?;
        assert!(matches!(
            frame.pop_layer(),
            Err(RenderError::UnbalancedLayer)
        ));
    }
    // The overlap shows the same color as either square alone: half red.
    let (single, overlap) = (pixel(&pixels, 12, 12), pixel(&pixels, 30, 30));
    assert_eq!(single, overlap);
    let mut expected = vec![0; SIZE * SIZE * 4];
    let mut surface = Surface::new(&mut expected, SIZE as u32, SIZE as u32)?;
    let half = fills(&[(rect(8.0, 8.0, 32.0, 32.0)?, Color::rgba(220, 30, 30, 128))])?;
    renderer
        .begin_frame(&mut surface, Color::WHITE)
        .draw(&half, Affine::IDENTITY)?;
    let direct = pixel(&expected, 12, 12);
    assert!((0..3).all(|c| single[c].abs_diff(direct[c]) <= 1));
    assert_eq!(pixel(&pixels, 2, 2), [255, 255, 255, 255]);

    // A layer left open is composited when the frame drops.
    let mut surface = Surface::new(&mut pixels, SIZE as u32, SIZE as u32)?;
    {
        let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
        frame.push_layer(&layer(rect(0.0, 0.0, 64.0, 64.0)?, 0.5, 0.0)?)?;
        frame.draw(&content, Affine::IDENTITY)?;
    }
    assert_eq!(pixel(&pixels, 30, 30), overlap);
    Ok(())
}

#[test]
fn backdrop_blur_stays_inside_its_shape() -> Result {
    // Black left half, white right half; the blur covers the middle band.
    let backdrop = fills(&[(rect(0.0, 0.0, 32.0, 64.0)?, Color::BLACK)])?;
    let band = rect(16.0, 16.0, 32.0, 32.0)?;
    let mut pixels = vec![0; SIZE * SIZE * 4];
    let mut surface = Surface::new(&mut pixels, SIZE as u32, SIZE as u32)?;
    let mut renderer = Renderer::default();
    {
        let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
        frame.draw(&backdrop, Affine::IDENTITY)?;
        frame.push_layer(&layer(band, 1.0, 4.0)?)?;
        frame.pop_layer()?;
    }
    // Symmetric about the edge, gray at it, nearly unchanged far from it.
    let (left, right) = (pixel(&pixels, 31, 32), pixel(&pixels, 32, 32));
    assert!(left[0] > 60 && left[0] < 200 && right[0] > left[0]);
    assert!(pixel(&pixels, 18, 32)[0] < 20 && pixel(&pixels, 46, 32)[0] > 240);
    // Outside the shape the edge stays hard.
    assert_eq!(pixel(&pixels, 31, 8), [0, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 32, 8), [255, 255, 255, 255]);

    // Over the effect budget the blur is skipped and counted; a layer fails.
    renderer.set_effect_budget(1024);
    let mut surface = Surface::new(&mut pixels, SIZE as u32, SIZE as u32)?;
    let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
    frame.draw(&backdrop, Affine::IDENTITY)?;
    let error = frame.push_layer(&layer(band, 1.0, 4.0)?).unwrap_err();
    assert!(matches!(error, RenderError::EffectBudget { .. }));
    drop(frame);
    assert_eq!(renderer.skipped_blurs(), 1);
    assert_eq!(pixel(&pixels, 31, 32), [0, 0, 0, 255]);
    Ok(())
}

#[test]
fn layers_nest_and_clip() -> Result {
    let green = fills(&[(rect(0.0, 0.0, 64.0, 64.0)?, Color::rgb(0, 160, 0))])?;
    let mut pixels = vec![0; SIZE * SIZE * 4];
    let mut surface = Surface::new(&mut pixels, SIZE as u32, SIZE as u32)?;
    let mut renderer = Renderer::default();
    let clipped = Layer::new(
        rect(0.0, 0.0, 64.0, 64.0)?,
        Affine::IDENTITY,
        Rect::new(8.0, 8.0, 48.0, 48.0),
        Some(Rect::new(0.0, 0.0, 32.0, 64.0)),
        1.0,
        0.0,
    )?;
    {
        let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
        frame.push_layer(&clipped)?;
        frame.push_layer(&layer(rect(0.0, 0.0, 64.0, 64.0)?, 0.5, 0.0)?)?;
        frame.draw(&green, Affine::IDENTITY)?;
        frame.pop_layer()?;
        frame.pop_layer()?;
    }
    // Only the extent within the clip shows, at half opacity.
    let inside = pixel(&pixels, 20, 20);
    assert!(inside[1] > 160 && inside[0] < 255 && inside[0] > 100);
    assert_eq!(pixel(&pixels, 4, 20), [255, 255, 255, 255]);
    assert_eq!(pixel(&pixels, 40, 20), [255, 255, 255, 255]);
    Ok(())
}
