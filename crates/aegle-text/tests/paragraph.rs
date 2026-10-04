//! Retained Unicode shaping and explicit font-policy boundaries.

use aegle_text::{Alignment, Blob, TextError, TextStyle, TextSystem};
use std::sync::Arc;

#[test]
fn explicit_font_policy_and_invalid_updates_preserve_retained_text() {
    let mut system = TextSystem::new();
    assert!(system.collection_mut().family_names().next().is_none());
    assert!(matches!(
        system.register_fonts(Blob::from(vec![0; 16])),
        Err(TextError::InvalidFont)
    ));
    let mut paragraph = system
        .paragraph("你好 Aegle", &TextStyle::default())
        .unwrap();
    assert_eq!(paragraph.diagnostics().unshaped_bytes, "你好 Aegle".len());
    assert_eq!(paragraph.missing_glyphs(), 0);
    #[cfg(feature = "scene")]
    assert_eq!(
        paragraph.paint(&mut aegle_scene::SceneBuilder::new()),
        Err(aegle_text::PaintError::MissingFont)
    );
    assert_eq!(
        paragraph.reflow(Some(f32::NAN), Alignment::Start),
        Err(TextError::InvalidWidth)
    );
    let invalid = TextStyle {
        size: 0.0,
        ..TextStyle::default()
    };
    assert_eq!(
        system.update(&mut paragraph, "rejected", &invalid),
        Err(TextError::InvalidStyle)
    );
    assert_eq!(paragraph.text(), "你好 Aegle");
    system
        .update(&mut paragraph, "", &TextStyle::default())
        .unwrap();
    assert_eq!(paragraph.diagnostics().unshaped_bytes, 0);
    paragraph.reflow(Some(0.0), Alignment::Center).unwrap();
    system.trim();
}

#[test]
fn cjk_reflow_reuses_font_and_reports_missing_glyphs() {
    let bytes = include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice();
    let blob = Blob::new(Arc::new(bytes));
    let mut system = TextSystem::new();
    system.register_fonts(blob.clone()).unwrap();
    let style = TextStyle {
        families: "Aegle Test CJK",
        locale: Some(aegle_text::Language::parse("zh-Hans").unwrap()),
        ..TextStyle::default()
    };
    let mut paragraph = system
        .paragraph("你好 é e\u{301}\n日本語 한글", &style)
        .unwrap();
    assert_eq!(paragraph.diagnostics(), Default::default());
    let intrinsic = paragraph.content_widths();
    let height = paragraph.size().height;
    assert!(
        paragraph
            .reflow(Some(40.0), Alignment::Start)
            .unwrap()
            .height
            > height
    );
    let mut glyphs = 0;
    paragraph.glyph_runs(|run| {
        assert_eq!(run.run().font().data.id(), blob.id());
        glyphs += run.positioned_glyphs().count();
    });
    assert!(glyphs > 10);
    paragraph.reflow(Some(40.0), Alignment::Justify).unwrap();
    assert_eq!(paragraph.content_widths().max, intrinsic.max);
    #[cfg(feature = "scene")]
    {
        let mut scene = aegle_scene::SceneBuilder::new();
        paragraph
            .paint_with_color(&mut scene, aegle_types::Color::WHITE)
            .unwrap();
        assert!(
            scene
                .finish()
                .unwrap()
                .glyph_runs()
                .iter()
                .all(|run| run.color() == aegle_types::Color::WHITE)
        );
    }
    system
        .update(&mut paragraph, "你好\u{10FFFF}", &style)
        .unwrap();
    assert_eq!(paragraph.diagnostics().unshaped_bytes, 0);
    assert_eq!(paragraph.missing_glyphs(), 1);
    system.trim();
    assert_eq!(paragraph.text(), "你好\u{10FFFF}");
    paragraph.reflow(None, Alignment::Start).unwrap();
    #[cfg(feature = "scene")]
    {
        let bold = TextStyle {
            weight: aegle_text::FontWeight::BOLD,
            ..style
        };
        system.restyle(&mut paragraph, &bold).unwrap();
        assert_eq!(
            paragraph.paint(&mut aegle_scene::SceneBuilder::new()),
            Err(aegle_text::PaintError::SyntheticStyle)
        );
    }
}
