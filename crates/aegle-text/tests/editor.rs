//! Integrated editing, composition and bounded-history lifecycle.

use aegle_text::{Alignment, Blob, EditorOptions, Selection, TextError, TextStyle, TextSystem};
use std::sync::Arc;

#[test]
fn composition_and_history_share_one_retained_editor() {
    let mut system = TextSystem::new();
    system
        .register_fonts(Blob::new(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        )))
        .unwrap();
    let style = TextStyle {
        families: "Aegle Test CJK",
        ..TextStyle::default()
    };
    let options = EditorOptions {
        multiline: true,
        ..Default::default()
    };
    let mut editor = system.editor("A你好B", &style, options).unwrap();
    let selection = |anchor, focus| Selection { anchor, focus };
    let selected = selection(7, 1);
    system.edit(&mut editor).select(selected).unwrap();
    editor.take_changes();
    system
        .edit(&mut editor)
        .set_preedit("中文", Some(selection(3, 3)))
        .unwrap();
    assert_eq!(editor.text(), "A你好B");
    assert_eq!(editor.display_text(), "A中文B");
    assert_eq!(editor.selection(), selected);
    assert_eq!(editor.selected_text(), "你好");
    assert!(!editor.take_changes().value);
    system
        .edit(&mut editor)
        .set_preedit("日本語", None)
        .unwrap();
    assert_eq!(editor.composition_range(), Some(1..10));
    assert!(editor.caret_rect(1.0).unwrap().is_none());
    editor.take_changes();
    assert_eq!(
        system
            .edit(&mut editor)
            .set_preedit("你好", Some(selection(1, 6))),
        Err(TextError::InvalidRange)
    );
    assert_eq!(editor.display_text(), "A日本語B");
    assert_eq!(editor.take_changes(), Default::default());
    system
        .edit(&mut editor)
        .reflow(Some(40.0), Alignment::Center)
        .unwrap();
    system
        .edit(&mut editor)
        .restyle(&TextStyle {
            size: 18.0,
            ..style.clone()
        })
        .unwrap();
    assert_eq!(editor.text(), "A你好B");
    assert_eq!(editor.composition_range(), Some(1..10));
    assert!(editor.caret_rect(1.0).unwrap().is_none());
    assert!(system.edit(&mut editor).cancel_preedit());
    assert_eq!(editor.display_text(), "A你好B");
    assert_eq!(editor.selection(), selected);
    assert!(editor.caret_rect(1.0).unwrap().is_some());
    assert!(!editor.take_changes().value);
    assert_eq!(editor.history_stats().undo_steps, 0);
    system.edit(&mut editor).set_preedit("中文", None).unwrap();
    system.edit(&mut editor).commit("世界").unwrap();
    assert_eq!(editor.text(), "A世界B");
    // Peeking leaves the changes for the host to drain.
    assert!(editor.changes().value);
    assert!(editor.take_changes().value);
    assert!(!editor.changes().value);
    assert_eq!(editor.history_stats().undo_steps, 1);
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "A你好B");
    assert_eq!(editor.selection(), selected);
    assert!(system.edit(&mut editor).redo().unwrap());
    assert_eq!(editor.text(), "A世界B");

    system.edit(&mut editor).set_text("").unwrap();
    system.edit(&mut editor).insert("a").unwrap();
    system.edit(&mut editor).insert("b").unwrap();
    assert_eq!(editor.history_stats().undo_steps, 1);
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "");
    assert!(system.edit(&mut editor).redo().unwrap());
    assert_eq!(editor.text(), "ab");
    system.edit(&mut editor).set_preedit("你", None).unwrap();
    editor.take_changes();
    system.edit(&mut editor).set_read_only(true);
    assert_eq!(editor.display_text(), "ab");
    assert_eq!(editor.composition_range(), None);
    assert!(!editor.take_changes().value);
    assert_eq!(
        system.edit(&mut editor).insert("a"),
        Err(TextError::ReadOnly)
    );
    assert_eq!(
        system.edit(&mut editor).set_preedit("你", None),
        Err(TextError::ReadOnly)
    );
    assert_eq!(system.edit(&mut editor).undo(), Err(TextError::ReadOnly));
    system.edit(&mut editor).set_read_only(false);

    system.edit(&mut editor).set_text("").unwrap();
    system.edit(&mut editor).insert("a").unwrap();
    let one_step = editor.history_stats().bytes;
    editor.set_history_limit(one_step);
    system.edit(&mut editor).insert("b").unwrap();
    assert!(editor.history_stats().bytes <= one_step);
    assert_eq!(editor.history_stats().undo_steps, 1);
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "a");
    assert!(!system.edit(&mut editor).undo().unwrap());
    editor.set_history_limit(0);
    system.edit(&mut editor).insert("c").unwrap();
    assert_eq!(editor.text(), "ac");
    assert_eq!(editor.history_stats(), Default::default());
    assert!(!system.edit(&mut editor).redo().unwrap());

    assert!(matches!(
        system.editor("a\nb", &style, EditorOptions::default()),
        Err(TextError::SingleLine)
    ));
    let mut single = system
        .editor("abc abc abc", &style, EditorOptions::default())
        .unwrap();
    assert_eq!(
        system.edit(&mut single).insert("\n"),
        Err(TextError::SingleLine)
    );
    assert_eq!(
        system.edit(&mut single).set_preedit("\u{2028}", None),
        Err(TextError::SingleLine)
    );
    system
        .edit(&mut single)
        .reflow(Some(1.0), Alignment::Start)
        .unwrap();
    assert_eq!(single.layout().len(), 1);
    assert_eq!(single.text(), "abc abc abc");
    assert!(single.ime_rect().size.width >= 0.0);

    editor.set_history_limit(EditorOptions::default().history_bytes);
    system.edit(&mut editor).set_text("e").unwrap();
    system.edit(&mut editor).insert("\u{301}").unwrap();
    assert_eq!(editor.text(), "e\u{301}");
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "e");
    assert!(system.edit(&mut editor).redo().unwrap());
    assert_eq!(editor.text(), "e\u{301}");
    system.edit(&mut editor).backspace().unwrap();
    assert_eq!(editor.text(), "");
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "e\u{301}");
}
