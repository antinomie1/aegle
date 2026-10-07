//! One explicit-adapter scenario: geometry, clips, blending, atlas recycling and
//! frame failure, checked against the software renderer.
#![cfg(feature = "text")]

use aegle_render_wgpu::{Error, Options, Renderer};
use aegle_scene::{Affine, Color, Rect, RoundedRect, Scene, SceneBuilder};
use aegle_text::{Blob, TextStyle, TextSystem};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const SIZE: u32 = 96;

fn gpu(renderer: &mut Renderer, scene: &Scene, clip: Option<Rect>) -> Result<Vec<u8>> {
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::rgb(250, 250, 250))?;
    frame.draw_clipped(scene, Affine::IDENTITY, clip)?;
    frame.finish()?;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    renderer.read_pixels(&mut pixels)?;
    Ok(pixels)
}

fn software(scene: &Scene, clip: Option<Rect>) -> Result<Vec<u8>> {
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    let mut surface = aegle_render_software::Surface::new(&mut pixels, SIZE, SIZE)?;
    aegle_render_software::Renderer::default()
        .begin_frame(&mut surface, Color::rgb(250, 250, 250))
        .draw_clipped(scene, Affine::IDENTITY, clip)?;
    Ok(pixels)
}

fn at(pixels: &[u8], x: u32, y: u32) -> &[u8] {
    &pixels[((y * SIZE + x) * 4) as usize..][..4]
}

#[test]
#[ignore = "requires a GPU adapter; select one with WGPU_BACKEND / WGPU_ADAPTER_NAME"]
fn geometry_text_atlas_recycling_and_failed_frames() -> Result {
    // A page must fit a bordered pixel; the default holds ordinary glyphs.
    assert!(matches!(
        Renderer::new(Options {
            atlas_size: 2,
            ..Options::default()
        }),
        Err(Error::InvalidSize)
    ));
    assert_eq!(Options::default().atlas_size, 1024);
    let mut renderer = Renderer::new(Options {
        // Holds a few CJK glyphs, so one text run clears the page mid-frame.
        atlas_size: 40,
        ..Options::default()
    })?;
    eprintln!("wgpu device: {}", renderer.device_name());

    // Pixel-aligned shapes keep samples away from differing antialiased edges.
    let mut builder = SceneBuilder::new();
    builder.fill(
        RoundedRect::new(Rect::new(8.0, 8.0, 40.0, 24.0), 4.0)?,
        Color::rgb(30, 90, 200),
    )?;
    builder.fill(
        RoundedRect::new(Rect::new(24.0, 16.0, 40.0, 24.0), 0.0)?,
        Color::rgba(220, 40, 40, 128),
    )?;
    builder.stroke(
        RoundedRect::new(Rect::new(12.0, 44.0, 48.0, 20.0), 0.0)?,
        Color::BLACK,
        2.0,
    )?;
    builder.push_clip(RoundedRect::new(Rect::new(64.0, 8.0, 24.0, 24.0), 0.0)?)?;
    builder.fill(
        RoundedRect::new(Rect::new(56.0, 0.0, 40.0, 40.0), 0.0)?,
        Color::rgb(20, 160, 90),
    )?;
    builder.pop()?;
    let shapes = builder.finish()?;
    let (first, expected) = (gpu(&mut renderer, &shapes, None)?, software(&shapes, None)?);
    for (x, y) in [
        (10, 20),
        (30, 20),
        (56, 30),
        (30, 54),
        (70, 12),
        (90, 12),
        (70, 36),
    ] {
        for (a, e) in at(&first, x, y).iter().zip(at(&expected, x, y)) {
            assert!(a.abs_diff(*e) <= 2, "({x}, {y}): GPU {a}, software {e}");
        }
    }
    assert_eq!(
        at(&first, 70, 12),
        [20, 160, 90, 255],
        "clip admits its interior"
    );
    assert_eq!(
        at(&first, 90, 12),
        [250, 250, 250, 255],
        "clip rejects the rest"
    );

    // Text takes the shared glyph rasterizer, so bytes agree beyond edge rounding.
    let mut fonts = TextSystem::new();
    fonts.register_fonts(Blob::from(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec(),
    ))?;
    let paragraph = fonts.paragraph(
        "中文日本語한글你好世界",
        &TextStyle {
            families: "Aegle Test CJK",
            size: 18.0,
            color: Color::rgba(32, 80, 170, 200),
            ..TextStyle::default()
        },
    )?;
    let mut builder = SceneBuilder::new();
    builder.push_transform(Affine::translation(4.25, 6.5)?)?;
    paragraph.paint(&mut builder)?;
    builder.pop()?;
    let text = builder.finish()?;
    let (actual, expected) = (gpu(&mut renderer, &text, None)?, software(&text, None)?);
    let inked = actual
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] != 250)
        .count();
    assert!(inked > 100, "text produced {inked} inked pixels");
    for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert!(a.abs_diff(*e) <= 3, "byte {i}: GPU {a}, software {e}");
    }

    // Unsupported depth poisons the frame; the renderer then draws normally again.
    let mut builder = SceneBuilder::new();
    for _ in 0..9 {
        builder.push_clip(RoundedRect::new(Rect::new(0.0, 0.0, 8.0, 8.0), 0.0)?)?;
    }
    for _ in 0..9 {
        builder.pop()?;
    }
    let deep = builder.finish()?;
    let mut frame = renderer.begin_frame(SIZE, SIZE, Color::WHITE)?;
    assert!(matches!(
        frame.draw(&deep, Affine::IDENTITY),
        Err(Error::ClipDepth)
    ));
    assert!(matches!(
        frame.draw(&shapes, Affine::IDENTITY),
        Err(Error::FrameFailed)
    ));
    assert!(matches!(frame.finish(), Err(Error::FrameFailed)));
    assert_eq!(
        gpu(&mut renderer, &shapes, None)?,
        first,
        "a failed frame leaves no residue"
    );
    Ok(())
}
