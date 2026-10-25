//! Retained Unicode shaping and explicit font-policy boundaries.

use aegle_text::{Alignment, TextError, TextStyle, TextSystem};
use std::sync::Arc;

#[test]
fn explicit_font_policy_and_invalid_updates_preserve_retained_text() {
    let mut system = TextSystem::new();
    // Without system discovery or registered fonts nothing can be shaped.
    let unfonted = system.paragraph("a", &TextStyle::default()).unwrap();
    assert!(unfonted.diagnostics().unshaped_bytes > 0);
    assert!(matches!(
        system.register_fonts(Arc::new(vec![0u8; 16])),
        Err(TextError::InvalidFont)
    ));
    let mut paragraph = system
        .paragraph("你好 Aegle", &TextStyle::default())
        .unwrap();
    assert_eq!(paragraph.diagnostics().unshaped_bytes, "你好 Aegle".len());
    assert_eq!(paragraph.missing_glyphs(), 0);
    #[cfg(feature = "scene")]
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            paragraph.paint(&mut aegle_scene::SceneBuilder::new())
        }))
        .is_err(),
        "painting text without a font panics"
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
    let blob = Arc::new(bytes);
    let mut system = TextSystem::new();
    system.register_fonts(blob.clone()).unwrap();
    let style = TextStyle {
        families: "Aegle Test CJK",
        locale: Some("zh-Hans"),
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
    assert!(paragraph.line_count() > 1);
    paragraph.reflow(Some(40.0), Alignment::Justify).unwrap();
    assert_eq!(paragraph.content_widths().max, intrinsic.max);
    #[cfg(feature = "scene")]
    {
        let mut scene = aegle_scene::SceneBuilder::new();
        paragraph.paint_with_color(&mut scene, aegle_types::Color::WHITE);
        assert!(
            scene
                .finish()
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
            weight: 700.0,
            ..style
        };
        // The fixture has only a regular face: bold is synthesized, while a
        // medium weight keeps the regular outlines, as in browsers.
        let mut emboldened = |style: &TextStyle<'_>| {
            system.restyle(&mut paragraph, style).unwrap();
            let mut builder = aegle_scene::SceneBuilder::new();
            paragraph.paint(&mut builder);
            let scene = builder.finish();
            assert!(!scene.glyph_runs().is_empty());
            scene.glyph_runs().iter().all(|run| run.embolden())
        };
        assert!(emboldened(&bold));
        let medium = TextStyle {
            weight: 500.0,
            ..style
        };
        assert!(!emboldened(&medium));
    }
}

#[test]
fn color_changes_skip_shaping_and_released_layouts_are_restored() {
    let mut system = TextSystem::new();
    system
        .register_fonts(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        ))
        .unwrap();
    let style = TextStyle {
        families: "Aegle Test CJK",
        ..TextStyle::default()
    };
    let mut paragraph = system.paragraph("你好 Aegle", &style).unwrap();
    let wrapped = paragraph.reflow(Some(30.0), Alignment::Center).unwrap();
    // The layout keeps the color it was shaped with: a color alone needs none.
    let red = aegle_types::Color::rgb(255, 0, 0);
    let recolored = TextStyle {
        color: red,
        ..style
    };
    system.restyle(&mut paragraph, &recolored).unwrap();
    assert_eq!(paragraph.color(), red);

    paragraph.release();
    assert!(paragraph.is_released() && paragraph.line_count() == 0);
    assert_eq!(
        paragraph.reflow(Some(30.0), Alignment::Center),
        Err(TextError::Released)
    );
    // A released paragraph stores new text and style without shaping.
    system
        .update(&mut paragraph, "你好 Aegle!", &recolored)
        .unwrap();
    assert!(paragraph.is_released());
    system.restore(&mut paragraph);
    assert!(!paragraph.is_released());
    assert_eq!(paragraph.text(), "你好 Aegle!");
    assert!(paragraph.size().height >= wrapped.height);

    // Hosts report idle text against the budget shared by every window.
    system.set_idle_budget(10);
    system.report_idle(0, 12);
    assert!(system.over_idle_budget());
    system.report_idle(12, 4);
    assert_eq!(system.idle_bytes(), 4);
    assert!(!system.over_idle_budget());
}
