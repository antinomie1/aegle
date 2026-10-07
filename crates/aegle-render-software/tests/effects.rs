//! Gradient fills interpolate in linear light and pad their ends; shadows fall
//! off around the shape within three standard deviations and respect clips.
use aegle_render_software::{Renderer, Surface};
use aegle_scene::{
    Affine, Color, Gradient, GradientStop, Point, Rect, RoundedRect, Scene, SceneBuilder,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn render(scene: &Scene) -> Result<Vec<u8>> {
    let mut pixels = vec![0; 64 * 64 * 4];
    let mut surface = Surface::new(&mut pixels, 64, 64)?;
    Renderer::default()
        .begin_frame(&mut surface, Color::WHITE)
        .draw(scene, Affine::IDENTITY)?;
    Ok(pixels)
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    pixels[(y * 64 + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn gradients_and_shadows() -> Result {
    let stops = [
        GradientStop {
            offset: 0.25,
            color: Color::rgb(255, 0, 0),
        },
        GradientStop {
            offset: 0.75,
            color: Color::rgb(0, 0, 255),
        },
    ];
    let linear = Gradient::linear(Point::new(0.0, 0.0), Point::new(64.0, 0.0), &stops)?;
    let mut builder = SceneBuilder::new();
    builder.fill_gradient(
        RoundedRect::new(Rect::new(0.0, 0.0, 64.0, 16.0), 0.0)?,
        &linear,
    )?;
    let pixels = render(&builder.finish()?)?;
    // Padded before the first and after the last stop, linear light between.
    assert_eq!(pixel(&pixels, 4, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 60, 8), [0, 0, 255, 255]);
    // At t = 32.5 / 64: 48.4% red and 51.6% blue in linear light.
    let middle = pixel(&pixels, 32, 8);
    assert!(middle[0].abs_diff(185) <= 1 && middle[2].abs_diff(190) <= 1);

    let mut builder = SceneBuilder::new();
    let shape = RoundedRect::new(Rect::new(16.0, 16.0, 32.0, 32.0), 0.0)?;
    builder.push_clip(RoundedRect::new(Rect::new(0.0, 0.0, 64.0, 40.0), 0.0)?)?;
    builder.shadow(shape, Color::BLACK, 4.0)?.pop()?;
    let pixels = render(&builder.finish()?)?;
    // Full strength inside, within the erf approximation.
    assert!(
        pixel(&pixels, 32, 32)[..3]
            .iter()
            .all(|channel| *channel <= 1)
    );
    // Half the shadow at the edge; nothing beyond three deviations.
    let edge = pixel(&pixels, 15, 32)[0];
    assert!((175..=200).contains(&edge), "{edge}");
    assert_eq!(pixel(&pixels, 3, 32), [255; 4]);
    assert!(pixel(&pixels, 12, 32)[0] > pixel(&pixels, 14, 32)[0]);
    assert_eq!(pixel(&pixels, 32, 44), [255; 4]);
    Ok(())
}

/// Draws `scene` into an RGBA and a BGRA surface over a colored background.
fn both_orders(scene: &Scene) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut renderer = Renderer::default();
    let background = Color::rgb(30, 90, 200);
    let (mut rgba, mut bgra) = (vec![0; 96 * 64 * 4], vec![0; 96 * 64 * 4]);
    let scale = Affine::scale(1.5, 1.5)?;
    renderer
        .begin_frame(&mut Surface::new(&mut rgba, 96, 64)?, background)
        .draw(scene, scale)?;
    renderer
        .begin_frame(&mut Surface::new_bgra(&mut bgra, 96, 64)?, background)
        .draw(scene, scale)?;
    Ok((rgba, bgra))
}

#[test]
fn bgra_surfaces_hold_the_same_pixels_with_red_and_blue_exchanged() -> Result {
    let shape = |x, y, w, h, r| RoundedRect::new(Rect::new(x, y, w, h), r);
    let stops = [
        GradientStop {
            offset: 0.0,
            color: Color::rgb(250, 120, 10),
        },
        GradientStop {
            offset: 1.0,
            color: Color::rgba(10, 200, 90, 160),
        },
    ];
    let gradient = Gradient::linear(Point::new(0.0, 0.0), Point::new(1.0, 1.0), &stops)?;
    let image = aegle_scene::Image::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 200, 100, 50, 255,
        ],
    )?;
    let mut square = aegle_scene::PathBuilder::new();
    square.move_to(Point::new(40.0, 4.0));
    square.line_to(Point::new(56.0, 6.0));
    square.line_to(Point::new(50.0, 20.0));
    square.close();
    let square = square.finish(aegle_scene::FillRule::NonZero)?;
    let mut builder = SceneBuilder::new();
    builder.shadow(
        shape(4.0, 4.0, 24.0, 16.0, 4.0)?,
        Color::rgba(0, 0, 0, 120),
        3.0,
    )?;
    builder.fill(shape(4.0, 4.0, 24.0, 16.0, 0.0)?, Color::rgb(220, 40, 60))?;
    builder.fill(
        shape(10.3, 8.6, 30.0, 20.0, 3.0)?,
        Color::rgba(40, 180, 90, 140),
    )?;
    builder.stroke(
        shape(2.5, 24.5, 40.0, 14.0, 0.0)?,
        Color::rgb(200, 20, 120),
        1.5,
    )?;
    builder.fill_gradient(shape(44.0, 24.0, 16.0, 14.0, 2.0)?, &gradient)?;
    builder.fill_path(&square, Color::rgb(255, 210, 0))?;
    builder.push_clip(shape(30.0, 2.0, 30.0, 40.0, 5.0)?)?;
    builder.image(&image, Rect::new(30.0, 28.0, 12.0, 12.0))?;
    builder.pop()?;
    builder.image(&image, Rect::new(2.0, 2.0, 2.0, 2.0))?;
    let (rgba, bgra) = both_orders(&builder.finish()?)?;
    for (a, b) in rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bgra.as_chunks::<4>().0.iter())
    {
        assert_eq!([a[2], a[1], a[0], a[3]], *b);
    }
    Ok(())
}
