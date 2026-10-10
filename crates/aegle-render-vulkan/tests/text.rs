//! One explicit-ICD scenario for shared CJK pixels, atlas residency and recovery.
#![cfg(feature = "text")]

use aegle_glyph::CacheLimits;
use aegle_render_vulkan::{Error, Options, Renderer, TextOptions};
use aegle_scene::{
    Affine, Color, FontData, Glyph, GlyphRun, Point, Rect, RoundedRect, Scene, SceneBuilder,
};
use aegle_text::{Blob, TextStyle, TextSystem};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn text(fonts: &mut TextSystem, value: &str, size: f32) -> Result<Scene> {
    let paragraph = fonts.paragraph(
        value,
        &TextStyle {
            families: "Aegle Test CJK",
            size,
            color: Color::rgba(32, 80, 170, 180),
            ..TextStyle::default()
        },
    )?;
    assert_eq!(paragraph.diagnostics(), Default::default());
    let mut builder = SceneBuilder::new();
    paragraph.paint(&mut builder);
    Ok(builder.finish())
}

fn compare(
    renderer: &mut Renderer,
    scene: &Scene,
    transform: Affine,
    clip: Option<Rect>,
    size: u32,
) -> Result<Vec<u8>> {
    let mut frame = renderer.begin_frame(size, size, Color::TRANSPARENT)?;
    frame.draw_clipped(scene, transform, clip)?;
    frame.finish()?;
    let mut pixels = vec![0; size as usize * size as usize * 4];
    renderer.read_pixels(&mut pixels)?;
    let mut expected = vec![0; pixels.len()];
    let mut surface = aegle_render_software::Surface::new(&mut expected, size, size)?;
    aegle_render_software::Renderer::default()
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw_clipped(scene, transform, clip)?;
    for (i, (&actual, &reference)) in pixels.iter().zip(&expected).enumerate() {
        assert!(
            actual.abs_diff(reference) <= 3,
            "byte {i}: GPU {actual}, software {reference}"
        );
    }
    Ok(pixels)
}

#[test]
#[ignore = "requires an explicitly selected Vulkan ICD/device"]
fn cjk_residency_color_filtering_and_atlas_recovery() -> Result {
    let mut fonts = TextSystem::new();
    fonts.register_fonts(Blob::from(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec(),
    ))?;
    let plain = text(&mut fonts, "中文 A", 20.0)?;
    let mut renderer = Renderer::new(Options {
        text: TextOptions {
            glyph_cache: CacheLimits {
                entries: 1,
                ..CacheLimits::default()
            },
            ..TextOptions::default()
        },
        ..Options::default()
    })?;
    eprintln!("Vulkan text device: {}", renderer.device_name());
    let transform = Affine::translation(4.25, 4.5);
    let clip = Some(Rect::new(4.0, 4.0, 52.0, 32.0));
    let first = compare(&mut renderer, &plain, transform, clip, 64)?;
    assert!(first.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count() > 100);
    let cold = renderer.text_stats();
    assert!(cold.raster_requests > 1 && cold.glyph_cache.entries == 1);
    assert_eq!(compare(&mut renderer, &plain, transform, clip, 64)?, first);
    // Only the pixel-less space, never resident in the atlas, asks the CPU cache.
    assert_eq!(
        renderer.text_stats().raster_requests,
        cold.raster_requests + 1
    );

    // Geometry before and after the text exercises pipeline/set switching in order.
    let mut frame = renderer.begin_frame(64, 64, Color::TRANSPARENT)?;
    let mut shapes = SceneBuilder::new();
    shapes.fill(
        RoundedRect::new(Rect::new(0.0, 0.0, 64.0, 64.0), 0.0),
        Color::WHITE,
    );
    frame.draw(&shapes.finish(), Affine::IDENTITY)?;
    frame.draw(&plain, transform)?;
    let mut marker = SceneBuilder::new();
    marker.fill(
        RoundedRect::new(Rect::new(8.0, 8.0, 8.0, 8.0), 0.0),
        Color::rgb(255, 0, 0),
    );
    frame.draw(&marker.finish(), Affine::IDENTITY)?;
    frame.finish()?;
    let mut pixels = vec![0; 64 * 64 * 4];
    renderer.read_pixels(&mut pixels)?;
    assert_eq!(&pixels[(12 * 64 + 12) * 4..][..4], &[255, 0, 0, 255]);
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[2] > p[0] && p[3] == 255)
    );
    let c = std::f32::consts::FRAC_1_SQRT_2;
    compare(
        &mut renderer,
        &plain,
        Affine::new([c, c, -c, c, 28.25, 0.5])?,
        Some(Rect::new(8.0, 8.0, 40.0, 40.0)),
        64,
    )?;

    let mut color = SceneBuilder::new();
    color.glyphs(GlyphRun::new(
        FontData::new(
            Blob::from(include_bytes!("../../../tests/assets/aegle-test-color.otf").to_vec()),
            0,
        ),
        16.0,
        Color::rgba(170, 60, 40, 128),
        vec![],
        vec![
            Glyph {
                id: 2,
                position: Point::new(0.0, 2.0),
            },
            Glyph {
                id: 1,
                position: Point::new(12.0, 20.0),
            },
        ],
    ));
    let color = color.finish();
    compare(
        &mut renderer,
        &color,
        Affine::new([-1.0, 0.0, 0.0, 1.0, 40.5, 4.0])?,
        None,
        64,
    )?;
    compare(
        &mut renderer,
        &color,
        Affine::new([c, c, -c, c, 28.5, 2.0])?,
        None,
        64,
    )?;

    // An abandoned frame may already have new atlas entries and pending transfers.
    let abandoned = text(&mut fonts, "你好", 23.0)?;
    {
        let mut frame = renderer.begin_frame(64, 64, Color::TRANSPARENT)?;
        frame.draw(&abandoned, Affine::IDENTITY)?;
    }
    compare(&mut renderer, &abandoned, Affine::IDENTITY, None, 64)?;
    let mut limited = Renderer::new(Options {
        text: TextOptions {
            page_size: 32,
            max_pages: 1,
            max_entries: 1,
            upload_bytes: 4096,
            glyph_cache: CacheLimits {
                entries: 1,
                ..CacheLimits::default()
            },
        },
        ..Options::default()
    })?;
    let a = text(&mut fonts, "你", 20.0)?;
    let b = text(&mut fonts, "好", 20.0)?;
    for (scene, size) in [(&a, 64), (&b, 64), (&a, 80)] {
        compare(&mut limited, scene, Affine::IDENTITY, None, size)?;
        assert_eq!(limited.text_stats().atlas_pages, 1);
        assert_eq!(limited.text_stats().atlas_entries, 1);
    }
    assert_eq!(limited.text_stats().raster_requests, 3);
    // A run needing more entries than the atlas holds is drawn in parts,
    // continuing at the glyph that did not fit, as wgpu does.
    let full = text(&mut fonts, "你好你好", 20.0)?;
    compare(&mut limited, &full, Affine::IDENTITY, None, 96)?;
    assert_eq!(limited.text_stats().atlas_entries, 1);
    // Only a glyph that cannot fit an empty atlas fails.
    let huge = text(&mut fonts, "你", 80.0)?;
    let mut frame = limited.begin_frame(64, 64, Color::TRANSPARENT)?;
    let error = frame.draw(&huge, Affine::IDENTITY).unwrap_err();
    assert!(matches!(error, Error::GlyphTooLarge), "{error}");
    assert!(matches!(frame.finish(), Err(Error::FrameFailed)));
    compare(&mut limited, &a, Affine::IDENTITY, None, 64)?;
    // A fully clipped glyph must not pin the single entry needed by visible text.
    let mut edge = SceneBuilder::new();
    for (size, position) in [(16.0, Point::new(0.0, 2.0)), (8.0, Point::new(3.0, 1.0))] {
        edge.glyphs(GlyphRun::new(
            color.glyph_runs()[0].font().clone(),
            size,
            Color::WHITE,
            vec![],
            vec![Glyph { id: 2, position }],
        ));
    }
    let edge_pixels = compare(
        &mut limited,
        &edge.finish(),
        Affine::IDENTITY,
        Some(Rect::new(2.0, 0.0, 2.0, 2.0)),
        8,
    )?;
    assert!(
        edge_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 0)
    );
    assert_eq!(limited.text_stats().atlas_entries, 1);
    limited.release_images()?;
    let empty = limited.text_stats();
    assert_eq!(
        (
            empty.atlas_pages,
            empty.atlas_entries,
            empty.atlas_bytes,
            empty.staging_bytes
        ),
        (0, 0, 0, 0)
    );
    compare(&mut limited, &a, Affine::IDENTITY, None, 64)?;
    Ok(())
}
