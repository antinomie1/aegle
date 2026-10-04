//! Real shaped CJK records share clipping, transforms, colors and glyph lifetime.
#![cfg(feature = "text")]

use aegle_render_software::{Renderer, Surface};
use aegle_scene::{
    Affine, Color, FontData, Glyph, GlyphRun, Point, Rect, RoundedRect, SceneBuilder,
};
use aegle_text::{Blob, TextStyle, TextSystem};

#[test]
fn retained_cjk_pixels_clip_recolor_and_transform() -> Result<(), Box<dyn std::error::Error>> {
    let mut fonts = TextSystem::new();
    fonts.register_fonts(Blob::from(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec(),
    ))?;
    let text = fonts.paragraph(
        "中文 A",
        &TextStyle {
            families: "Aegle Test CJK",
            size: 20.0,
            ..TextStyle::default()
        },
    )?;
    assert_eq!(text.diagnostics(), Default::default());
    let mut pixels = vec![0; 64 * 48 * 4];
    let mut renderer = Renderer::default();
    let mut surface = Surface::new(&mut pixels, 64, 48)?;
    let mut warm = None;
    for color in [Color::rgb(0, 0, 255), Color::rgb(255, 0, 0)] {
        let mut builder = SceneBuilder::new();
        builder.push_clip(RoundedRect::new(Rect::new(4.0, 4.0, 36.0, 28.0), 4.0)?)?;
        builder.push_transform(Affine::translation(4.25, 4.5)?)?;
        text.paint_with_color(&mut builder, color)?;
        builder.pop()?.pop()?;
        let scene = builder.finish()?;
        renderer
            .begin_frame(&mut surface, Color::TRANSPARENT)
            .draw(&scene, Affine::IDENTITY)?;
        assert!(surface.data().chunks_exact(4).filter(|p| p[3] != 0).count() > 100);
        for (i, pixel) in surface.data().chunks_exact(4).enumerate() {
            let (x, y) = (i % 64, i / 64);
            if !(4..40).contains(&x) || !(4..32).contains(&y) {
                assert_eq!(pixel, [0; 4]);
            }
            let channel = if color.to_rgba()[0] == 0 { 2 } else { 0 };
            assert_eq!(pixel[channel], pixel[3]);
            assert!(pixel[..3].iter().all(|value| *value <= pixel[3]));
        }
        let stats = renderer.glyph_cache().stats();
        if let Some(previous) = warm {
            assert_eq!(stats, previous);
        }
        warm = Some(stats);
        // Repeating a retained record neither changes pixels nor grows its cache.
        let first = surface.data().to_vec();
        renderer
            .begin_frame(&mut surface, Color::TRANSPARENT)
            .draw(&scene, Affine::IDENTITY)?;
        assert_eq!(surface.data(), first);
    }
    let mut builder = SceneBuilder::new();
    text.paint(&mut builder)?;
    let scene = builder.finish()?;
    renderer
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&scene, Affine::new([0.0, 1.0, -1.0, 0.0, 40.0, 0.0])?)?;
    assert!(surface.data().chunks_exact(4).any(|p| p[3] > 200));
    // Font data lives in the retained record, independent of the shaping context.
    drop(text);
    drop(fonts);
    renderer.glyph_cache_mut().clear();
    renderer
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&scene, Affine::IDENTITY)?;
    assert!(renderer.glyph_cache().stats().image_bytes > 0);
    let mut text_only = Renderer::new(0);
    text_only
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&scene, Affine::IDENTITY)?;
    assert_eq!(text_only.allocated_mask_bytes(), 0);
    renderer
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&scene, Affine::new([1.0, 0.0, 0.0, 1e-10, 0.0, 0.01])?)?;
    assert!(surface.data().iter().all(|v| *v == 0));

    // Fixture B (glyph 2) is a 2×2 color strike. Reflect by half a pixel to
    // mix translucent red with green; linear filtering gives ~[117,160,0,192].
    let font = FontData::new(
        Blob::from(include_bytes!("../../../tests/assets/aegle-test-color.otf").to_vec()),
        0,
    );
    let mut builder = SceneBuilder::new();
    builder.glyphs(GlyphRun::new(
        font,
        16.0,
        Color::WHITE,
        vec![],
        vec![Glyph {
            id: 2,
            position: Point::new(0.0, 2.0),
        }],
    )?)?;
    let color_scene = builder.finish()?;
    renderer
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&color_scene, Affine::new([-1.0, 0.0, 0.0, 1.0, 2.5, 0.0])?)?;
    for (actual, expected) in surface.data()[4..8].iter().zip([117, 160, 0, 192]) {
        assert!(actual.abs_diff(expected) <= 1);
    }
    let c = std::f32::consts::FRAC_1_SQRT_2;
    renderer
        .begin_frame(&mut surface, Color::TRANSPARENT)
        .draw(&color_scene, Affine::new([c, c, -c, c, 2.5, 1.0])?)?;
    // A one-byte fringe lies outside the unfiltered bitmap bounds.
    assert!(surface.data()[(2 * 64 + 4) * 4 + 3] > 0);
    Ok(())
}

#[test]
fn editor_decorations_follow_composition_and_hidden_caret() -> Result<(), Box<dyn std::error::Error>>
{
    use aegle_text::{EditorOptions, EditorPaint, Selection};
    let mut fonts = TextSystem::new();
    fonts.register_fonts(Blob::from(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").to_vec(),
    ))?;
    let mut editor = fonts.editor(
        "你好",
        &TextStyle {
            families: "Aegle Test CJK",
            size: 20.0,
            ..Default::default()
        },
        EditorOptions::default(),
    )?;
    fonts.edit(&mut editor).select(Selection {
        anchor: 0,
        focus: 6,
    })?;
    let mut pixels = vec![0; 80 * 48 * 4];
    let mut surface = Surface::new(&mut pixels, 80, 48)?;
    let mut renderer = Renderer::default();
    for visible in [false, true] {
        fonts
            .edit(&mut editor)
            .set_preedit("世界", visible.then_some(Selection::default()))?;
        let mut builder = SceneBuilder::new();
        builder.push_transform(Affine::translation(4.0, 4.0)?)?;
        editor.paint(
            &mut builder,
            EditorPaint {
                caret: Some(Color::rgb(255, 0, 0)),
                preedit: Some(Color::rgb(0, 255, 0)),
                ..Default::default()
            },
        )?;
        builder.pop()?;
        renderer
            .begin_frame(&mut surface, Color::WHITE)
            .draw(&builder.finish()?, Affine::IDENTITY)?;
        assert_eq!(
            surface
                .data()
                .chunks_exact(4)
                .any(|p| p == [255, 0, 0, 255]),
            visible
        );
        assert!(
            surface
                .data()
                .chunks_exact(4)
                .any(|p| p == [0, 255, 0, 255])
        );
        assert_eq!(editor.text(), "你好");
    }
    fonts.edit(&mut editor).cancel_preedit();
    assert_eq!(editor.selected_text(), "你好");
    let mut builder = SceneBuilder::new();
    editor.paint(
        &mut builder,
        EditorPaint {
            selection: Some(Color::rgb(0, 0, 255)),
            caret: None,
            ..Default::default()
        },
    )?;
    renderer
        .begin_frame(&mut surface, Color::WHITE)
        .draw(&builder.finish()?, Affine::IDENTITY)?;
    assert!(
        surface
            .data()
            .chunks_exact(4)
            .any(|p| p == [0, 0, 255, 255])
    );
    Ok(())
}
