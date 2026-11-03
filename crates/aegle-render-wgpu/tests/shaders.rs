//! Custom fragment programs: a program's color fills its shape like a fill
//! of that color, `time` and `values` reach it, programs interleave with
//! other draws, and one that does not compile fails the frame.

use aegle_render_wgpu::{Error, Options, Renderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, Scene, SceneBuilder, Shader};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: u32 = 64;
const VALUES: &str = "fn shade(uv: vec2f, size: vec2f) -> vec4f {\n\
    return vec4f(values[0], values[1], values[2], values[3]);\n}";

fn render(renderer: &mut Renderer, scene: &Scene) -> Result<Vec<u8>> {
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::TRANSPARENT)?;
    frame.draw(scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    Ok(pixels)
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    pixels[i..i + 4].try_into().unwrap()
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn programs_fill_like_their_color() -> Result {
    let mut renderer = Renderer::new(Options::default())?;
    let shape = RoundedRect::with_radii(Rect::new(6.0, 8.0, 50.0, 40.0), [14.0, 0.0, 6.0, 20.0]);
    let program = Shader::new(VALUES);
    let mut values = [0.0; Shader::VALUES];
    values[..4].copy_from_slice(&[0.2, 0.6, 0.9, 0.75]);
    let mut builder = SceneBuilder::new();
    builder
        .push_transform(Affine::new([0.9, 0.2, -0.2, 0.9, 8.0, 2.0])?)
        .shade(shape, &program, 0.0, values)
        .pop();
    let shaded = render(&mut renderer, &builder.finish())?;
    let mut builder = SceneBuilder::new();
    builder
        .push_transform(Affine::new([0.9, 0.2, -0.2, 0.9, 8.0, 2.0])?)
        .fill(shape, Color::rgba(51, 153, 230, 191))
        .pop();
    let filled = render(&mut renderer, &builder.finish())?;
    let worst = shaded
        .iter()
        .zip(&filled)
        .map(|(a, b)| a.abs_diff(*b))
        .max();
    assert!(worst.unwrap() <= 2, "worst channel difference {worst:?}");
    assert_eq!(pixel(&shaded, 30, 30)[3], 191);
    assert_eq!(pixel(&shaded, 1, 1), [0; 4]);
    Ok(())
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn time_uv_and_programs_between_other_draws() -> Result {
    let mut renderer = Renderer::new(Options::default())?;
    let clock = Shader::new(
        "fn shade(uv: vec2f, size: vec2f) -> vec4f { return vec4f(fract(time), 0.0, 0.0, 1.0); }",
    );
    let ramp = Shader::new(
        "fn shade(uv: vec2f, size: vec2f) -> vec4f { return vec4f(0.0, uv.x, size.y / 100.0, 1.0); }",
    );
    let square = |x| RoundedRect::new(Rect::new(x, 0.0, 16.0, 16.0), 0.0);
    let mut builder = SceneBuilder::new();
    builder
        .shade(square(0.0), &clock, 2.5, [0.0; 8])
        .fill(square(16.0), Color::rgb(0, 0, 255))
        .shade(square(32.0), &ramp, 0.0, [0.0; 8])
        .shade(square(48.0), &clock, 7.0, [0.0; 8]);
    let pixels = render(&mut renderer, &builder.finish())?;
    // fract(2.5) is sRGB 0.5; fract(7.0) is black.
    assert!(pixel(&pixels, 8, 8)[0].abs_diff(128) <= 1);
    assert_eq!(pixel(&pixels, 24, 8), [0, 0, 255, 255]);
    let [_, left, depth, _] = pixel(&pixels, 33, 8);
    let [_, right, _, _] = pixel(&pixels, 46, 8);
    assert!(left < 30 && right > 220, "{left} {right}");
    assert!(depth.abs_diff(41) <= 1, "size reaches the program: {depth}");
    assert_eq!(pixel(&pixels, 56, 8), [0, 0, 0, 255]);
    Ok(())
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn a_program_that_does_not_compile_fails_the_frame() -> Result {
    let mut renderer = Renderer::new(Options::default())?;
    let broken = Shader::new("fn shade(uv: vec2f, size: vec2f) -> vec4f { return nope; }");
    let mut builder = SceneBuilder::new();
    builder.shade(
        RoundedRect::new(Rect::new(0.0, 0.0, 8.0, 8.0), 0.0),
        &broken,
        0.0,
        [0.0; 8],
    );
    let scene = builder.finish();
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::TRANSPARENT)?;
    match frame.draw(&scene, Affine::IDENTITY) {
        Err(Error::Shader(error)) => assert_eq!(error.location.map(|l| l.0), Some(1)),
        other => panic!("{other:?}"),
    }
    // The renderer still draws what compiles.
    let mut builder = SceneBuilder::new();
    builder.shade(
        RoundedRect::new(Rect::new(0.0, 0.0, 8.0, 8.0), 0.0),
        &Shader::new(VALUES),
        0.0,
        [1.0; 8],
    );
    assert_eq!(
        pixel(&render(&mut renderer, &builder.finish())?, 4, 4),
        [255; 4]
    );
    Ok(())
}
