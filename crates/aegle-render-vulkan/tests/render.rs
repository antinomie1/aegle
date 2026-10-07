//! Explicit Vulkan/ICD integration; interior samples avoid backend-specific AA edges.
use aegle_render_vulkan::{Error, Options, Renderer};
use aegle_scene::{
    Affine, Blob, Color, FontData, Glyph, GlyphRun, Point, Rect, RoundedRect, SceneBuilder,
};

fn shape(x: f32, y: f32, w: f32, h: f32, radius: f32) -> RoundedRect {
    RoundedRect::new(Rect::new(x, y, w, h), radius).unwrap()
}

fn near(pixels: &[u8], x: usize, y: usize, expected: [u8; 4]) {
    let actual = &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "pixel ({x}, {y}): {actual:?}, expected {expected:?}"
    );
}

#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn geometry_boundaries_and_failed_frame_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = Renderer::new(Options::default())?;
    eprintln!("Vulkan device: {}", renderer.device_name());
    let mut builder = SceneBuilder::new();
    builder.push_transform(Affine::translation(4.0, 4.0)?)?;
    builder.push_clip(shape(0.0, 0.0, 24.0, 24.0, 4.0))?;
    builder.push_transform(Affine::new([0.0, 1.0, -1.0, 0.0, 24.0, 0.0])?)?;
    builder.push_clip(shape(4.0, 4.0, 12.0, 12.0, 2.0))?;
    builder.fill(
        shape(-8.0, -8.0, 40.0, 40.0, 0.0),
        Color::rgba(0, 0, 0, 128),
    )?;
    builder.pop()?.pop()?;
    builder.stroke(shape(2.0, 2.0, 20.0, 20.0, 0.0), Color::rgb(0, 0, 255), 2.0)?;
    builder.pop()?.pop()?;
    builder.fill(shape(0.0, 30.0, 4.0, 4.0, 0.0), Color::rgb(255, 0, 0))?;
    let scene = builder.finish()?;
    let mut pixels = vec![0; 64 * 64 * 4];
    for _ in 0..2 {
        let mut frame = renderer.begin_frame(64, 64, Color::WHITE)?;
        frame.draw(&scene, Affine::IDENTITY)?;
        frame.finish()?;
        renderer.read_pixels(&mut pixels)?;
        near(&pixels, 16, 12, [187, 187, 187, 255]);
        near(&pixels, 4, 4, [255; 4]);
        near(&pixels, 6, 12, [0, 0, 255, 255]);
        near(&pixels, 3, 12, [255; 4]);
        near(&pixels, 1, 31, [255, 0, 0, 255]);
    }
    let mut marker = SceneBuilder::new();
    marker.fill(shape(40.0, 40.0, 8.0, 8.0, 0.0), Color::rgb(0, 255, 0))?;
    let marker = marker.finish()?;
    let mut frame = renderer.begin_frame(64, 64, Color::WHITE)?;
    frame.draw_clipped(
        &scene,
        Affine::translation(2.0, 0.0)?,
        Some(Rect::new(14.0, 8.0, 12.0, 10.0)),
    )?;
    frame.draw(&marker, Affine::IDENTITY)?;
    frame.finish()?;
    renderer.read_pixels(&mut pixels)?;
    near(&pixels, 18, 12, [187, 187, 187, 255]);
    near(&pixels, 12, 12, [255; 4]);
    near(&pixels, 18, 20, [255; 4]);
    near(&pixels, 43, 43, [0, 255, 0, 255]);

    let base = Color::rgba(110, 100, 220, 128);
    let mut blend = SceneBuilder::new();
    blend.fill(
        shape(0.0, 0.0, 32.0, 32.0, 0.0),
        Color::rgba(220, 200, 40, 160),
    )?;
    let blend = blend.finish()?;
    let mut frame = renderer.begin_frame(64, 64, base)?;
    frame.draw(&blend, Affine::IDENTITY)?;
    frame.finish()?;
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_software::Surface::new(&mut expected, 64, 64)?;
    aegle_render_software::Renderer::default()
        .begin_frame(&mut surface, base)
        .draw(&blend, Affine::IDENTITY)?;
    near(
        &pixels,
        12,
        12,
        expected[(12 * 64 + 12) * 4..][..4].try_into()?,
    );
    near(&pixels, 50, 50, [55, 50, 110, 128]);
    assert!(renderer.read_pixels(&mut [0; 4]).is_err());
    assert!(renderer.begin_frame(0, 64, Color::WHITE).is_err());
    assert!(
        renderer
            .begin_frame(u32::MAX, u32::MAX, Color::WHITE)
            .is_err()
    );
    assert!(
        Renderer::new(Options {
            memory_budget: 1,
            ..Options::default()
        })
        .and_then(|mut r| r.begin_frame(64, 64, Color::WHITE)?.finish())
        .is_err()
    );
    let mut deep = SceneBuilder::new();
    for _ in 0..9 {
        deep.push_clip(shape(0.0, 0.0, 32.0, 32.0, 2.0))?;
    }
    deep.fill(shape(0.0, 0.0, 32.0, 32.0, 0.0), Color::BLACK)?;
    for _ in 0..9 {
        deep.pop()?;
    }
    let mut text = SceneBuilder::new();
    text.glyphs(GlyphRun::new(
        FontData::new(Blob::new(std::sync::Arc::new(Vec::<u8>::new())), 0),
        16.0,
        Color::BLACK,
        vec![],
        vec![Glyph {
            id: 1,
            position: Point::new(8.0, 16.0),
        }],
    )?)?;
    for rejected in [deep.finish()?, text.finish()?] {
        let mut frame = renderer.begin_frame(64, 64, Color::WHITE)?;
        assert!(frame.draw(&rejected, Affine::IDENTITY).is_err());
        assert!(matches!(frame.finish(), Err(Error::FrameFailed)));
    }
    {
        let mut frame = renderer.begin_frame(64, 64, Color::WHITE)?;
        assert!(
            frame
                .draw_clipped(
                    &scene,
                    Affine::IDENTITY,
                    Some(Rect::new(f32::NAN, 0.0, 1.0, 1.0))
                )
                .is_err()
        );
    }
    let mut frame = renderer.begin_frame(64, 64, Color::WHITE)?;
    frame.draw(&marker, Affine::IDENTITY)?;
    frame.finish()?;
    renderer.wait()?;
    renderer.read_pixels(&mut pixels)?;
    near(&pixels, 43, 43, [0, 255, 0, 255]);
    near(&pixels, 16, 12, [255; 4]);
    let stats = renderer.stats();
    assert!(stats.device_bytes > 0 && stats.device_bytes <= Options::default().memory_budget);
    assert!(stats.recording_bytes <= 2 << 20);
    renderer.begin_frame(16, 8, Color::BLACK)?.finish()?;
    let mut resized = vec![0; 16 * 8 * 4];
    renderer.read_pixels(&mut resized)?;
    assert!(resized.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 255]));
    renderer.release_images()?;
    assert_eq!(renderer.stats().device_bytes, 0);
    assert_eq!(renderer.stats().recording_bytes, 0);
    assert!(renderer.read_pixels(&mut resized).is_err());
    renderer.begin_frame(16, 8, Color::WHITE)?.finish()?;
    renderer.read_pixels(&mut resized)?;
    assert!(resized.iter().all(|&channel| channel == 255));
    Ok(())
}

/// A scene with more records than one submission holds is drawn in parts, in
/// painter's order, keeping clips that span the parts.
#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn large_frames_are_split_into_submissions() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = SceneBuilder::new();
    builder.push_clip(shape(0.0, 0.0, 56.0, 64.0, 0.0))?;
    // 40,000 opaque pixels over 64×64: each pixel is painted about ten times,
    // last in pass nine (blue) before pixel 3136 and in pass eight (red) after.
    for index in 0..40_000_u32 {
        let pixel = index % 4096;
        let color = if (index / 4096) % 2 == 1 {
            Color::rgb(0, 0, 255)
        } else {
            Color::rgb(255, 0, 0)
        };
        let (x, y) = ((pixel % 64) as f32, (pixel / 64) as f32);
        builder.fill(shape(x, y, 1.0, 1.0, 0.0), color)?;
    }
    builder.pop()?;
    let scene = builder.finish()?;
    let mut renderer = Renderer::new(Options::default())?;
    let mut frame = renderer.begin_frame(64, 64, Color::WHITE)?;
    frame.draw(&scene, Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; 64 * 64 * 4];
    renderer.read_pixels(&mut pixels)?;
    near(&pixels, 0, 0, [0, 0, 255, 255]);
    near(&pixels, 55, 48, [0, 0, 255, 255]);
    near(&pixels, 0, 49, [255, 0, 0, 255]);
    near(&pixels, 60, 10, [255; 4]);
    assert!(renderer.stats().recording_bytes <= 2 << 20);
    Ok(())
}
