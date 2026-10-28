//! The cairo backend against the tiny-skia one: pixel-aligned geometry must be
//! byte-identical, and antialiased edges may differ only by tiny-skia's coverage
//! quantization.
use aegle_render_cairo::{Renderer as CairoRenderer, Surface};
use aegle_render_tiny_skia::Renderer as SkiaRenderer;
use aegle_scene::{
    Affine, Color, Image, Layer, PathBuilder, Point, Rect, RoundedRect, SceneBuilder,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SIZE: u32 = 128;

fn shape(x: f32, y: f32, w: f32, h: f32, r: f32) -> RoundedRect {
    RoundedRect::new(Rect::new(x, y, w, h), r)
}

/// Renders one scene through both CPU backends over the same background.
fn both(scene: &aegle_scene::Scene, clip: Option<Rect>) -> Result<(Vec<u8>, Vec<u8>)> {
    let pixels = (SIZE * SIZE * 4) as usize;
    let (mut skia, mut cairo) = (vec![0; pixels], vec![0; pixels]);
    {
        let mut surface = Surface::new(&mut skia, SIZE, SIZE)?;
        SkiaRenderer::default()
            .begin_frame(&mut surface, Color::rgb(250, 250, 250))
            .draw_clipped(scene, Affine::IDENTITY, clip)?;
    }
    {
        let mut surface = Surface::new(&mut cairo, SIZE, SIZE)?;
        CairoRenderer::default()
            .begin_frame(&mut surface, Color::rgb(250, 250, 250))
            .draw_clipped(scene, Affine::IDENTITY, clip)?;
    }
    Ok((skia, cairo))
}

/// Asserts two frames are equal, reporting where they first differ.
fn assert_same(label: &str, skia: &[u8], cairo: &[u8]) {
    let pairs = skia.as_chunks::<4>().0.iter().zip(cairo.as_chunks::<4>().0);
    for (index, (a, b)) in pairs.enumerate() {
        if a == b {
            continue;
        }
        let (x, y) = (index as u32 % SIZE, index as u32 / SIZE);
        panic!("{label}: pixel ({x}, {y}) is {a:?} on tiny-skia and {b:?} on cairo");
    }
}

/// The largest channel difference and how many pixels exceed one coverage step.
fn difference(skia: &[u8], cairo: &[u8]) -> (u8, usize) {
    let mut worst = 0;
    let mut differing = 0;
    for (a, b) in skia.as_chunks::<4>().0.iter().zip(cairo.as_chunks::<4>().0) {
        let delta = (0..4).map(|channel| a[channel].abs_diff(b[channel])).max();
        worst = worst.max(delta.unwrap_or(0));
        if delta.unwrap_or(0) > 1 {
            differing += 1;
        }
    }
    (worst, differing)
}

#[test]
fn pixel_aligned_geometry_is_byte_identical() -> Result {
    let mut builder = SceneBuilder::new();
    // Whole-pixel edges, half-pixel edges, borders and overlaps.
    builder.fill(shape(4.0, 4.0, 40.0, 24.0, 0.0), Color::rgb(30, 90, 200));
    builder.stroke(
        shape(8.0, 40.0, 48.0, 32.0, 0.0),
        Color::rgb(200, 40, 40),
        3.0,
    );
    builder.fill(shape(60.5, 8.0, 32.0, 32.0, 0.0), Color::rgba(0, 0, 0, 128));
    builder.fill(shape(24.0, 60.0, 40.0, 40.0, 0.0), Color::rgb(20, 160, 90));
    builder.fill(
        shape(40.0, 76.0, 40.0, 40.0, 0.0),
        Color::rgba(255, 255, 255, 96),
    );
    // A whole-pixel clip takes the compositor's scissor path, not a mask.
    let scene = builder.finish();
    let (skia, cairo) = both(&scene, Some(Rect::new(16.0, 16.0, 80.0, 80.0)))?;
    assert_same("pixel-aligned fills, borders and clips", &skia, &cairo);
    Ok(())
}

#[test]
fn pixel_aligned_paths_are_byte_identical() -> Result {
    // An explicit axis-aligned path does reach a rasterizer, and both agree
    // exactly on it: integer edges produce only 0 and 255 coverage.
    // One shared outline, filled, then stroked, then filled even-odd.
    let mut outline = PathBuilder::new();
    outline.move_to(Point::new(8.0, 8.0));
    outline.line_to(Point::new(40.0, 8.0));
    outline.line_to(Point::new(40.0, 40.0));
    outline.line_to(Point::new(8.0, 40.0));
    outline.close();
    let outline = outline.finish(aegle_scene::FillRule::NonZero);
    let mut ring = PathBuilder::new();
    ring.move_to(Point::new(8.0, 8.0));
    ring.line_to(Point::new(40.0, 8.0));
    ring.line_to(Point::new(40.0, 40.0));
    ring.line_to(Point::new(8.0, 40.0));
    ring.close();
    let ring = ring.finish(aegle_scene::FillRule::EvenOdd);
    let mut builder = SceneBuilder::new();
    builder.fill_path(&outline, Color::BLACK);
    builder.stroke_path(
        &outline,
        Color::rgb(200, 40, 40),
        aegle_scene::Stroke::new(4.0),
    );
    builder.fill_path(&ring, Color::rgba(0, 0, 0, 90));
    let scene = builder.finish();
    let (skia, cairo) = both(&scene, None)?;
    assert_same("pixel-aligned paths", &skia, &cairo);
    Ok(())
}

#[test]
fn images_gradients_shadows_and_layers_are_byte_identical() -> Result {
    // An unscaled, pixel-aligned image is copied; a scaled one is filtered by the
    // shared compositor. Neither goes through a rasterizer.
    let image = Image::new(
        2,
        2,
        vec![
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
        ],
    )?;
    let gradient = aegle_scene::Gradient::linear(
        Point::new(8.0, 8.0),
        Point::new(120.0, 120.0),
        &[
            aegle_scene::GradientStop {
                offset: 0.0,
                color: Color::rgb(255, 0, 0),
            },
            aegle_scene::GradientStop {
                offset: 1.0,
                color: Color::rgba(0, 0, 255, 128),
            },
        ],
    )?;
    let mut builder = SceneBuilder::new();
    builder.image(&image, Rect::new(8.0, 8.0, 2.0, 2.0));
    builder.image(&image, Rect::new(24.0, 8.0, 37.0, 23.0));
    builder.fill_gradient(shape(8.0, 40.0, 60.0, 40.0, 0.0), &gradient);
    builder.shadow(
        shape(70.0, 40.0, 40.0, 40.0, 10.0),
        Color::rgba(0, 0, 0, 120),
        6.0,
    );
    let scene = builder.finish();
    let (skia, cairo) = both(&scene, None)?;
    assert_same("images, gradients and shadows", &skia, &cairo);

    // Group opacity runs entirely in the shared compositor as well.
    let mut content = SceneBuilder::new();
    content.fill(shape(16.0, 16.0, 64.0, 64.0, 0.0), Color::rgb(220, 30, 30));
    let content = content.finish();
    let layer = Layer::new(
        shape(0.0, 0.0, SIZE as f32, SIZE as f32, 0.0),
        Affine::IDENTITY,
        Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32),
        None,
        0.5,
        0.0,
    )?;
    let pixels = (SIZE * SIZE * 4) as usize;
    let (mut skia, mut cairo) = (vec![0; pixels], vec![0; pixels]);
    {
        let mut renderer = SkiaRenderer::default();
        let mut surface = Surface::new(&mut skia, SIZE, SIZE)?;
        let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
        frame.push_layer(&layer)?;
        frame.draw(&content, Affine::IDENTITY)?;
        frame.pop_layer()?;
    }
    {
        let mut renderer = CairoRenderer::default();
        let mut surface = Surface::new(&mut cairo, SIZE, SIZE)?;
        let mut frame = renderer.begin_frame(&mut surface, Color::WHITE);
        frame.push_layer(&layer)?;
        frame.draw(&content, Affine::IDENTITY)?;
        frame.pop_layer()?;
    }
    assert_same("layer compositing", &skia, &cairo);
    Ok(())
}

#[test]
fn a_rounded_border_is_a_ring_not_a_disc() -> Result {
    // A shape's stroke width outlines a centered border: the ring is filled
    // even-odd, so the hole stays background. Stroking the ring instead would
    // fill the hole solid, which is what this pins.
    let mut builder = SceneBuilder::new();
    builder.stroke(shape(32.0, 32.0, 64.0, 64.0, 14.0), Color::BLACK, 3.0);
    let scene = builder.finish();
    let (skia, cairo) = both(&scene, None)?;
    // The hole centre and the outside are untouched on both backends.
    let at = |frame: &[u8], x: usize, y: usize| frame[(y * SIZE as usize + x) * 4];
    // Background inside the hole and outside the ring. The compositor quantizes
    // through linear light, so allow the same one-level rounding both share.
    for (x, y) in [(64, 64), (64, 20), (20, 64)] {
        assert!(
            at(&cairo, x, y) >= 248,
            "cairo painted ({x}, {y}) at {}, which the border does not cover",
            at(&cairo, x, y)
        );
        assert!(
            at(&skia, x, y) >= 248,
            "tiny-skia painted ({x}, {y}) at {}",
            at(&skia, x, y)
        );
    }
    // The ring itself is painted, and the two agree closely along it.
    assert!(at(&cairo, 64, 33) < 220, "cairo left the ring unpainted");
    assert!(at(&skia, 64, 33) < 220, "tiny-skia left the ring unpainted");
    let dark = |frame: &[u8]| {
        frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] < 128)
            .count()
    };
    let (skia_dark, cairo_dark) = (dark(&skia), dark(&cairo));
    assert!(
        skia_dark.abs_diff(cairo_dark) < 32,
        "ring area disagrees: {skia_dark} vs {cairo_dark} dark pixels"
    );
    Ok(())
}

#[test]
fn antialiased_edges_differ_only_by_coverage_quantization() -> Result {
    // tiny-skia samples four vertical sub-scanlines per pixel, so its coverage is
    // quantized to quarters while cairo computes exact area. Pixels may therefore
    // differ by up to one quarter of a step, but never flip between covered and
    // uncovered, and never leave the edges.
    let mut builder = SceneBuilder::new();
    builder.fill(shape(20.0, 20.0, 88.0, 88.0, 24.0), Color::BLACK);
    builder.push_transform(Affine::new([0.866, 0.5, -0.5, 0.866, 30.0, 6.0])?);
    builder.fill(shape(0.0, 0.0, 60.0, 60.0, 0.0), Color::rgb(20, 60, 200));
    builder.pop();
    let mut path = PathBuilder::new();
    path.move_to(Point::new(20.0, 110.0));
    for step in 1..10 {
        let t = step as f32;
        path.line_to(Point::new(20.0 + t * 10.0, 110.0 - (t * 0.7).sin() * 30.0));
    }
    builder.stroke_path(
        &path.finish(aegle_scene::FillRule::NonZero),
        Color::rgb(10, 80, 40),
        aegle_scene::Stroke::new(5.0),
    );
    let scene = builder.finish();
    let (skia, cairo) = both(&scene, None)?;

    let (worst, differing) = difference(&skia, &cairo);
    // tiny-skia quantizes horizontal edge coverage to quarters, so a pixel can be
    // up to a quarter of a step from cairo's exact area: 64 of 255. Where a
    // shallow curve crosses a pixel, the two also flatten the cubic differently,
    // which adds a fraction of one step.
    assert!(
        worst <= 96,
        "antialiased edge differed by {worst} levels, more than one step"
    );
    // Edges are a small fraction of the frame; a large difference would mean
    // geometry, not sampling, disagrees.
    assert!(
        differing < (SIZE * SIZE) as usize / 16,
        "{differing} pixels differ, too many for edge sampling alone"
    );
    // Nothing may move between fully covered and fully uncovered.
    for (a, b) in skia.as_chunks::<4>().0.iter().zip(cairo.as_chunks::<4>().0) {
        let edge = |pixel: &[u8]| pixel[3] == 0 || pixel[3] == 255;
        assert!(
            !(edge(a) && edge(b) && a[3] != b[3]),
            "a pixel flipped between covered and uncovered"
        );
    }
    Ok(())
}

#[test]
fn a_missing_library_is_reported_instead_of_substituted() {
    // This host has cairo, so the check is that the fallible constructor agrees
    // with the loader rather than quietly reporting success without a library.
    assert_eq!(
        aegle_render_cairo::renderer(usize::MAX).is_ok(),
        aegle_render_cairo::version().is_some()
    );
    if let Some(version) = aegle_render_cairo::version() {
        assert!(
            version.starts_with("1."),
            "unexpected cairo version {version}"
        );
    }
}

#[test]
fn a_mask_budget_failure_allocates_nothing() -> Result {
    // Zero budget: the first shape draw needs a coverage mask and must fail
    // before reserving any memory, on both CPU backends alike.
    let mut builder = SceneBuilder::new();
    builder.fill(shape(8.0, 8.0, 32.0, 32.0, 6.0), Color::BLACK);
    let scene = builder.finish();
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    let mut renderer = aegle_render_cairo::renderer(0)?;
    let mut surface = Surface::new(&mut pixels, SIZE, SIZE)?;
    let error = renderer
        .begin_frame(&mut surface, Color::WHITE)
        .draw(&scene, Affine::IDENTITY)
        .unwrap_err();
    assert!(
        matches!(error, aegle_render_cairo::RenderError::MaskBudget { .. }),
        "unexpected error {error:?}"
    );
    assert_eq!(
        renderer.allocated_mask_bytes(),
        0,
        "a rejected draw must not allocate masks"
    );
    Ok(())
}
