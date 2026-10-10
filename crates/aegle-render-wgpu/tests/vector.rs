//! Images and paths match the software renderer, reuse uploads and recycle
//! exact-size textures.
#![cfg(feature = "text")]

use aegle_render_wgpu::{Options, Renderer};
use aegle_scene::{
    Affine, Color, FillRule, Image, LineCap, LineJoin, PathBuilder, Point, Rect, Scene,
    SceneBuilder, Stroke,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: u32 = 96;

/// Returns GPU pixels after checking them against the software renderer. The two
/// rasterizers place curve and stroke edges slightly differently, so only the
/// mean channel difference is bounded.
fn compare(renderer: &mut Renderer, scene: &Scene, transform: Affine) -> Result<Vec<u8>> {
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::WHITE)?;
    frame.draw(scene, transform)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_software::Surface::new(&mut expected, SIZE, SIZE)?;
    aegle_render_software::Renderer::default()
        .begin_frame(&mut surface, Color::WHITE)
        .draw(scene, transform)?;
    let total: u64 = pixels
        .iter()
        .zip(&expected)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    let mean = total as f64 / pixels.len() as f64;
    eprintln!("mean GPU/software channel difference: {mean:.3}");
    assert!(mean < 0.5);
    Ok(pixels)
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn images_and_paths_match_software_reuse_and_recycle() -> Result {
    let texels = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 128],
        [255, 255, 0, 255],
        [0, 0, 0, 0],
        [40, 90, 200, 255],
    ];
    let image = Image::new(3, 2, texels.concat())?;
    let mut star = PathBuilder::new();
    star.move_to(Point::new(20.0, 2.0));
    for i in 1..5 {
        let angle = i as f32 * 4.0 * std::f32::consts::PI / 5.0;
        star.line_to(Point::new(
            20.0 + 18.0 * angle.sin(),
            20.0 - 18.0 * angle.cos(),
        ));
    }
    star.close();
    let star = star.finish(FillRule::EvenOdd);
    let mut wave = PathBuilder::new();
    wave.move_to(Point::new(4.0, 70.0))
        .quad_to(Point::new(24.0, 50.0), Point::new(44.0, 70.0))
        .cubic_to(
            Point::new(54.0, 90.0),
            Point::new(70.0, 50.0),
            Point::new(88.0, 70.0),
        );
    let wave = wave.finish(FillRule::NonZero);
    let stroke = Stroke {
        width: 4.0,
        cap: LineCap::Round,
        join: LineJoin::Round,
    };
    let mut builder = SceneBuilder::new();
    builder
        .image(&image, Rect::new(2.0, 2.0, 3.0, 2.0))
        .image(&image, Rect::new(8.25, 2.5, 21.0, 10.0))
        .push_transform(Affine::translation(40.0, 4.0))
        .fill_path(&star, Color::rgba(200, 40, 90, 220))
        .pop()
        .stroke_path(&wave, Color::rgb(20, 120, 60), stroke);
    let scene = builder.finish();

    // An 80-pixel page holds the images and star masks but not the wave masks or the
    // 80-pixel image below, which receive exact-size textures.
    let mut renderer = Renderer::new(Options {
        atlas_size: 80,
        ..Options::default()
    })?;
    eprintln!("wgpu vector device: {}", renderer.device_name());
    let pixels = compare(&mut renderer, &scene, Affine::IDENTITY)?;
    // Unscaled images copy texels exactly; the semi-transparent one blends over white.
    let at = |x: usize, y: usize| &pixels[(y * SIZE as usize + x) * 4..][..4];
    assert_eq!(at(2, 2), [255, 0, 0, 255]);
    assert_eq!(at(4, 3), [40, 90, 200, 255]);
    assert_eq!(at(3, 3), [255, 255, 255, 255]);
    assert_eq!(
        renderer.resident_entries(),
        3,
        "one image and two path masks"
    );

    // Whole-pixel moves reuse masks; a new linear transform rasterizes again.
    compare(&mut renderer, &scene, Affine::translation(3.0, -2.0))?;
    assert_eq!(renderer.resident_entries(), 3);
    compare(
        &mut renderer,
        &scene,
        Affine::new([0.9, 0.2, -0.2, 0.9, 6.0, 0.0])?,
    )?;
    assert_eq!(renderer.resident_entries(), 5);

    // An image larger than a page gets its own texture. The 88-pixel wave masks
    // already did, so both kinds are released once two frames skip them.
    let large = Image::new(80, 10, [90, 30, 160, 255].repeat(800))?;
    let mut builder = SceneBuilder::new();
    builder.image(&large, Rect::new(4.0, 80.0, 80.0, 10.0));
    let pixels = compare(&mut renderer, &builder.finish(), Affine::IDENTITY)?;
    assert_eq!(
        &pixels[(84 * SIZE as usize + 40) * 4..][..4],
        [90, 30, 160, 255]
    );
    for _ in 0..2 {
        compare(&mut renderer, &scene, Affine::IDENTITY)?;
    }
    assert_eq!(
        renderer.resident_entries(),
        4,
        "image, star masks and wave remain; unused exact-size textures were released"
    );
    Ok(())
}
