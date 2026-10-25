//! An editor shapes each hard line on its own: after any sequence of edits
//! its geometry equals that of an editor freshly built from the same text,
//! and motion, deletion and selection cross line breaks as in one layout.

use aegle_text::{Alignment, Editor, EditorOptions, Movement, Selection, TextStyle, TextSystem};
use std::sync::Arc;

fn system() -> TextSystem {
    let mut system = TextSystem::new();
    system
        .register_fonts(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        ))
        .unwrap();
    system
}

fn style() -> TextStyle<'static> {
    TextStyle {
        families: "Aegle Test CJK",
        ..TextStyle::default()
    }
}

const MULTILINE: EditorOptions = EditorOptions {
    multiline: true,
    history_bytes: 1 << 20,
};

/// Asserts `editor` lays out as a new editor of its text with its selection.
fn same_as_fresh(system: &mut TextSystem, editor: &Editor, width: Option<f32>) {
    let text = editor.display_text().to_owned();
    let mut fresh = system.editor(&text, &style(), MULTILINE).unwrap();
    let mut driver = system.edit(&mut fresh);
    driver.reflow(width, Alignment::Left).unwrap();
    driver.select(editor.selection()).unwrap();
    assert_eq!(editor.size(), fresh.size(), "{text:?}");
    assert_eq!(editor.caret_rect(1.0), fresh.caret_rect(1.0), "{text:?}");
    let rects = |editor: &Editor| {
        let mut rects = Vec::new();
        editor.selection_rects(|rect| rects.push(rect));
        rects
    };
    assert_eq!(rects(editor), rects(&fresh), "{text:?}");
    assert_eq!(editor.line_count(), fresh.line_count(), "{text:?}");
}

#[test]
fn incremental_lines_match_a_fresh_layout() {
    let mut system = system();
    let mut editor = system
        .editor("Aegle 你好\nB\n\nAB 你好 AB", &style(), MULTILINE)
        .unwrap();
    let width = Some(60.0);
    system
        .edit(&mut editor)
        .reflow(width, Alignment::Left)
        .unwrap();
    let at = |index| Selection {
        anchor: index,
        focus: index,
    };
    same_as_fresh(&mut system, &editor, width);

    // Typing in a line and splitting it.
    system.edit(&mut editor).select(at(3)).unwrap();
    system.edit(&mut editor).insert("你好你好").unwrap();
    same_as_fresh(&mut system, &editor, width);
    system.edit(&mut editor).insert("\n").unwrap();
    same_as_fresh(&mut system, &editor, width);
    assert_eq!(editor.caret_rect(1.0).unwrap().unwrap().origin.x, 0.0);

    // Backspace at a line start removes the break and joins the lines.
    system.edit(&mut editor).backspace().unwrap();
    same_as_fresh(&mut system, &editor, width);
    assert_eq!(editor.text(), "Aeg你好你好le 你好\nB\n\nAB 你好 AB");

    // A carriage return before a line feed becomes one CRLF break.
    let feed = editor.display_text().find('\n').unwrap();
    system.edit(&mut editor).replace(feed..feed, "\r").unwrap();
    same_as_fresh(&mut system, &editor, width);
    system.edit(&mut editor).select(at(feed + 2)).unwrap();
    system.edit(&mut editor).backspace().unwrap();
    assert_eq!(editor.text(), "Aeg你好你好le 你好B\n\nAB 你好 AB");
    same_as_fresh(&mut system, &editor, width);

    // Deleting a selection across breaks, then undoing and redoing it.
    let selection = Selection {
        anchor: 2,
        focus: editor.display_text().len() - 3,
    };
    system.edit(&mut editor).select(selection).unwrap();
    same_as_fresh(&mut system, &editor, width);
    system.edit(&mut editor).delete().unwrap();
    assert_eq!(editor.text(), "Ae AB");
    same_as_fresh(&mut system, &editor, width);
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "Aeg你好你好le 你好B\n\nAB 你好 AB");
    same_as_fresh(&mut system, &editor, width);
    assert!(system.edit(&mut editor).redo().unwrap());
    same_as_fresh(&mut system, &editor, None);

    // Rewrapping and recoloring keep the geometry of a fresh editor.
    system.edit(&mut editor).set_text("A\nB\nAB").unwrap();
    system
        .edit(&mut editor)
        .reflow(None, Alignment::Left)
        .unwrap();
    same_as_fresh(&mut system, &editor, None);
    editor.take_changes();
    let red = TextStyle {
        color: aegle_types::Color::rgb(255, 0, 0),
        ..style()
    };
    system.edit(&mut editor).restyle(&red).unwrap();
    let changes = editor.take_changes();
    assert!(changes.paint && !changes.layout);
}

#[test]
fn motion_crosses_line_breaks() {
    let mut system = system();
    let mut editor = system.editor("AB\n\n你好", &style(), MULTILINE).unwrap();
    let caret = |editor: &Editor| editor.selection().focus;
    let mut go = |editor: &mut Editor, movement| {
        system.edit(editor).move_cursor(movement, false).unwrap();
        caret(editor)
    };
    assert_eq!(go(&mut editor, Movement::TextStart), 0);
    assert_eq!(go(&mut editor, Movement::LineEnd), 2);
    assert_eq!(go(&mut editor, Movement::Right), 3);
    assert_eq!(go(&mut editor, Movement::Right), 4);
    assert_eq!(go(&mut editor, Movement::Left), 3);
    assert_eq!(go(&mut editor, Movement::Down), 4);
    assert_eq!(go(&mut editor, Movement::Up), 3);
    assert_eq!(go(&mut editor, Movement::Up), 0);
    assert_eq!(go(&mut editor, Movement::Up), 0);
    assert_eq!(
        go(&mut editor, Movement::TextEnd),
        editor.display_text().len()
    );
    assert_eq!(go(&mut editor, Movement::Down), editor.display_text().len());
    // Hit testing finds the line by height.
    let below = editor.size().height - 1.0;
    system
        .edit(&mut editor)
        .select_at(
            aegle_types::Point::new(0.0, below),
            aegle_text::HitSelection::Caret,
        )
        .unwrap();
    assert_eq!(caret(&editor), 4);
}
