//! Pixel-level integration boundaries: clipping, compositing and bounded reuse.
use aegle_render_software::{RenderError, Renderer, Surface};
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};

fn shape(x: f32, y: f32, w: f32, h: f32, r: f32) -> RoundedRect {
    RoundedRect::new(Rect::new(x, y, w, h), r).unwrap()
}

#[test]
fn nested_clips_transform_borders_and_mask_reuse() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = SceneBuilder::new();
    builder.push_transform(Affine::translation(4.0, 4.0)?)?;
    builder.push_clip(shape(0.0, 0.0, 24.0, 24.0, 4.0))?;
    builder.push_transform(Affine::translation(4.0, 4.0)?)?;
    builder.push_clip(shape(0.0, 0.0, 12.0, 12.0, 0.0))?;
    builder.fill(
        shape(-8.0, -8.0, 32.0, 32.0, 0.0),
        Color::rgba(0, 0, 0, 128),
    )?;
    builder.pop()?.pop()?;
    builder.stroke(shape(2.0, 2.0, 20.0, 20.0, 0.0), Color::rgb(0, 0, 255), 2.0)?;
    builder.pop()?.pop()?;
    builder.fill(shape(0.0, 30.0, 2.0, 2.0, 0.0), Color::rgb(255, 0, 0))?;
    let scene = builder.finish()?;
    let mut pixels = vec![0; 32 * 32 * 4];
    let mut renderer = Renderer::new(3 * 32 * 32);
    let mut surface = Surface::new(&mut pixels, 32, 32)?;
    for _ in 0..2 {
        renderer
            .begin_frame(&mut surface, Color::WHITE)
            .draw(&scene, Affine::IDENTITY)?;
        let pixel = |x: usize, y: usize| &surface.data()[(y * 32 + x) * 4..(y * 32 + x + 1) * 4];
        assert_eq!(pixel(12, 12), [187, 187, 187, 255]);
        assert_eq!(pixel(22, 12), [255, 255, 255, 255]);
        assert_eq!(pixel(4, 4), [255, 255, 255, 255]);
        assert_eq!(pixel(6, 12), [0, 0, 255, 255]);
        assert_eq!(pixel(0, 30), [255, 0, 0, 255]);
        assert_eq!(renderer.allocated_mask_bytes(), 3 * 32 * 32);
    }
    let mut small = vec![0; 16 * 16 * 4];
    renderer
        .begin_frame(&mut Surface::new(&mut small, 16, 16)?, Color::WHITE)
        .draw(&scene, Affine::scale(0.5, 0.5)?)?;
    assert_eq!(renderer.allocated_mask_bytes(), 3 * 16 * 16);
    let mut limited = Renderer::new(1);
    assert_eq!(
        limited
            .begin_frame(&mut surface, Color::WHITE)
            .draw(&scene, Affine::IDENTITY),
        Err(RenderError::MaskBudget {
            required: 3072,
            limit: 1
        })
    );
    assert_eq!(limited.allocated_mask_bytes(), 0);
    renderer.release_scratch();
    assert_eq!(renderer.allocated_mask_bytes(), 0);
    Ok(())
}

#[test]
fn transparent_linear_compositing_and_rotated_records() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = SceneBuilder::new();
    builder.fill(
        shape(0.0, 0.0, 2.0, 4.0, 0.0),
        Color::rgba(255, 255, 255, 128),
    )?;
    let scene = builder.finish()?;
    let mut pixels = [0; 4 * 4 * 4];
    let mut renderer = Renderer::default();
    let mut surface = Surface::new(&mut pixels, 4, 4)?;
    renderer
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&scene, Affine::new([0.0, 1.0, -1.0, 0.0, 4.0, 0.0])?)?;
    assert_eq!(&surface.data()[..4], &[128, 128, 128, 128]);
    assert_eq!(&surface.data()[32..36], &[0, 0, 0, 0]);

    let mut builder = scene.into_builder();
    builder.clear();
    let base = Color::rgba(110, 100, 220, 128);
    let top = Color::rgba(220, 200, 40, 160);
    builder.fill(shape(0.0, 0.0, 4.0, 4.0, 0.0), top)?;
    renderer
        .begin_frame(&mut surface, base)
        .draw(&builder.finish()?, Affine::IDENTITY)?;
    // Reference transfer functions, independent of the renderer's interpolation tables.
    let decode = |v: f64| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let encode = |v: f64| {
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    let alpha = 160.0 / 255.0 + 128.0 / 255.0 * (1.0 - 160.0 / 255.0);
    for (i, (dst, src)) in [55.0, 50.0, 110.0]
        .into_iter()
        .zip([220.0, 200.0, 40.0])
        .enumerate()
    {
        let linear = (decode(src / 255.0) * 160.0 / 255.0
            + decode(dst / 128.0) * 128.0 / 255.0 * (1.0 - 160.0 / 255.0))
            / alpha;
        let expected = (encode(linear) * alpha * 255.0).round() as u8;
        assert!(surface.data()[i].abs_diff(expected) <= 1);
    }
    assert_eq!(surface.data()[3], (alpha * 255.0).round() as u8);
    Ok(())
}
