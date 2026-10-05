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
    let clip = Some(Rect::new(12.0, 10.0, 14.0, 6.0));
    assert_eq!(
        renderer
            .begin_frame(&mut surface, Color::WHITE)
            .draw_clipped(&scene, Affine::IDENTITY, clip),
        Err(RenderError::MaskBudget {
            required: 4 * 32 * 32,
            limit: 3 * 32 * 32
        })
    );
    let mut clipped = Renderer::new(4 * 32 * 32);
    clipped
        .begin_frame(&mut surface, Color::WHITE)
        .draw_clipped(&scene, Affine::translation(2.0, 0.0)?, clip)?;
    for (x, y, expected) in [(12, 12, 187), (10, 12, 255), (24, 12, 255), (12, 8, 255)] {
        assert_eq!(surface.data()[(y * 32 + x) * 4], expected);
    }
    assert_eq!(clipped.allocated_mask_bytes(), 4 * 32 * 32);
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

    {
        let mut frame = renderer.begin_frame(&mut surface, Color::TRANSPARENT);
        frame.draw_clipped(
            &scene,
            Affine::translation(1.0, 0.0)?,
            Some(Rect::new(1.0, 0.0, 1.0, 1.0)),
        )?;
        frame.draw(&scene, Affine::translation(2.0, 2.0)?)?;
    }
    assert_eq!(&surface.data()[4..8], &[128; 4]);
    assert_eq!(&surface.data()[8..12], &[0; 4]);
    assert_eq!(&surface.data()[40..44], &[128; 4]); // Next draw is not clipped.
    for clip in [Rect::new(0.0, 0.0, 0.0, 4.0), Rect::new(0.0, 0.0, 4.0, 0.0)] {
        renderer
            .begin_frame(&mut surface, Color::TRANSPARENT)
            .draw_clipped(&scene, Affine::IDENTITY, Some(clip))?;
        assert!(surface.data().iter().all(|v| *v == 0));
    }
    let empty = SceneBuilder::new().finish()?;
    for clip in [
        Rect::new(f32::NAN, 0.0, 1.0, 1.0),
        Rect::new(0.0, 0.0, -1.0, 1.0),
        Rect::new(1_048_577.0, 0.0, 0.0, 1.0),
    ] {
        assert_eq!(
            renderer
                .begin_frame(&mut surface, Color::TRANSPARENT)
                .draw_clipped(&empty, Affine::IDENTITY, Some(clip)),
            Err(RenderError::Coordinates)
        );
    }

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

#[test]
fn paths_match_rect_coverage_and_images_copy_texels() -> Result<(), Box<dyn std::error::Error>> {
    use aegle_scene::{FillRule, Image, PathBuilder, Point, Stroke};
    let mut square = PathBuilder::new();
    square
        .move_to(Point::new(1.5, 1.0))
        .line_to(Point::new(9.0, 1.0))
        .line_to(Point::new(9.0, 6.25))
        .line_to(Point::new(1.5, 6.25))
        .close();
    let square = square.finish(FillRule::NonZero)?;
    let color = Color::rgba(30, 140, 200, 200);
    let render = |scene: &aegle_scene::Scene| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut pixels = vec![0; 16 * 16 * 4];
        Renderer::default()
            .begin_frame(&mut Surface::new(&mut pixels, 16, 16)?, Color::WHITE)
            .draw(scene, Affine::IDENTITY)?;
        Ok(pixels)
    };
    let mut builder = SceneBuilder::new();
    builder.fill_path(&square, color)?;
    let path = render(&builder.finish()?)?;
    let mut builder = SceneBuilder::new();
    builder.fill(shape(1.5, 1.0, 7.5, 5.25, 0.0), color)?;
    assert_eq!(path, render(&builder.finish()?)?);

    let image = Image::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128])?;
    let mut line = PathBuilder::new();
    line.move_to(Point::new(0.0, 12.5))
        .line_to(Point::new(16.0, 12.5));
    let mut builder = SceneBuilder::new();
    builder
        .image(&image, Rect::new(3.0, 8.0, 2.0, 1.0))?
        .image(&image, Rect::new(8.0, 8.0, 8.0, 2.0))?
        .stroke_path(
            &line.finish(FillRule::NonZero)?,
            Color::BLACK,
            Stroke::new(1.0),
        )?;
    let pixels = render(&builder.finish()?)?;
    let at = |x: usize, y: usize| &pixels[(y * 16 + x) * 4..][..4];
    assert_eq!(at(3, 8), [255, 0, 0, 255]);
    assert_eq!(at(4, 8), [187, 187, 255, 255]);
    assert_eq!(at(8, 9), [255, 0, 0, 255]);
    assert_eq!(at(15, 9), [187, 187, 255, 255]); // Scaled edges clamp, not fade.
    assert_eq!(at(6, 12), [0, 0, 0, 255]);
    assert_eq!(at(6, 11), [255; 4]);
    Ok(())
}
