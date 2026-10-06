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
