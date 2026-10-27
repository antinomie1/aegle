//! Click positions and caret steps stay on UTF-8 boundaries for every script.
//!
//! The window host publishes the selection as IME surrounding offsets, which
//! the platform layer rejects unless both endpoints split characters.

use aegle_text::{Alignment, EditorOptions, HitSelection, Movement, TextStyle, TextSystem};
use aegle_types::Point;

fn system() -> TextSystem {
    let mut fonts = TextSystem::new();
    let families = fonts
        .register_fonts(std::sync::Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        ))
        .unwrap();
    fonts.set_generic_families(aegle_text::GenericFamily::SansSerif, families);
    fonts
}

fn style() -> TextStyle<'static> {
    TextStyle {
        families: "sans-serif",
        size: 16.0,
        ..TextStyle::default()
    }
}

#[test]
fn clicking_and_stepping_keep_caret_offsets_on_character_boundaries() {
    let mut fonts = system();
    let text = "你好世界 Aegle é";
    let mut editor = fonts
        .editor(text, &style(), EditorOptions::default())
        .unwrap();
    fonts
        .edit(&mut editor)
        .reflow(Some(200.0), Alignment::Start)
        .unwrap();
    let size = editor.size();

    // Every horizontal position, including the trailing edge, must land on a
    // boundary; three CJK characters are three bytes each.
    for step in 0..=40 {
        let x = size.width * step as f32 / 40.0;
        let y = size.height * 0.5;
        fonts
            .edit(&mut editor)
            .select_at(Point::new(x, y), HitSelection::Caret)
            .unwrap();
        let focus = editor.selection().focus;
        assert!(text.is_char_boundary(focus), "x={x} produced byte {focus}");
    }

    fonts
        .edit(&mut editor)
        .select_at(Point::new(0.0, 1.0), HitSelection::Caret)
        .unwrap();
    for _ in 0..24 {
        fonts
            .edit(&mut editor)
            .move_cursor(Movement::Right, false)
            .unwrap();
        let focus = editor.selection().focus;
        assert!(text.is_char_boundary(focus), "step produced byte {focus}");
    }
    for _ in 0..24 {
        fonts
            .edit(&mut editor)
            .move_cursor(Movement::Left, false)
            .unwrap();
        let focus = editor.selection().focus;
        assert!(text.is_char_boundary(focus), "step produced byte {focus}");
    }
    // The whole text is reachable and the surrounding excerpt validates.
    let surrounding = editor.surrounding(usize::MAX).unwrap();
    assert_eq!(surrounding.to_string(), text);
    assert!(text.is_char_boundary(surrounding.selection.focus));
}
