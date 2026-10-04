//! Atomic native input transactions and borrowed surrounding text.
use aegle_text::{Blob, EditorOptions, ImeEdit, Selection, TextError, TextStyle, TextSystem};
use std::sync::Arc;

#[test]
fn atomic_ime_batches_share_selection_history_and_bounded_surrounding() {
    let mut system = TextSystem::new();
    system
        .register_fonts(Blob::new(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        )))
        .unwrap();
    let style = TextStyle {
        families: "Aegle Test CJK",
        ..Default::default()
    };
    let mut editor = system
        .editor("é你好B", &style, EditorOptions::default())
        .unwrap();
    let selection = |anchor, focus| Selection { anchor, focus };
    let selected = selection(8, 2);
    system.edit(&mut editor).select(selected).unwrap();
    assert!(editor.surrounding(5).is_none());
    let preedit = ImeEdit {
        preedit: "中",
        ..Default::default()
    };
    system.edit(&mut editor).apply_ime(preedit).unwrap();
    let surrounding = editor.surrounding(3).unwrap();
    assert_eq!(surrounding.to_string(), "éB");
    assert_eq!(surrounding.selection, selection(2, 2));
    assert_eq!(editor.surrounding(usize::MAX).unwrap().parts(), ["é", "B"]);
    assert!(editor.surrounding(0).unwrap().is_empty());
    let commit = ImeEdit {
        delete_before: 2,
        delete_after: 1,
        commit: Some("世"),
        preedit: "界",
        cursor: None,
    };
    editor.take_changes();
    for invalid in 0..4 {
        let mut edit = commit;
        match invalid {
            0 => edit.preedit = "\n",
            1 => edit.delete_before = 1,
            2 => edit.delete_after = usize::MAX,
            _ => edit.cursor = Some(selection(1, 3)),
        }
        let error = if invalid == 0 {
            TextError::SingleLine
        } else {
            TextError::InvalidRange
        };
        assert_eq!(system.edit(&mut editor).apply_ime(edit), Err(error));
        assert_eq!(editor.display_text(), "é中B");
        assert_eq!(editor.text(), "é你好B");
        assert_eq!(editor.take_changes(), Default::default());
        assert_eq!(editor.history_stats().undo_steps, 0);
    }
    system.edit(&mut editor).apply_ime(commit).unwrap();
    assert_eq!(editor.text(), "世");
    assert_eq!(editor.display_text(), "世界");
    assert_eq!(editor.history_stats().undo_steps, 1);
    assert!(system.edit(&mut editor).cancel_preedit());
    assert!(system.edit(&mut editor).undo().unwrap());
    assert_eq!(editor.text(), "é你好B");
    assert_eq!(editor.selection(), selected);
    assert!(system.edit(&mut editor).redo().unwrap());
    assert_eq!(editor.text(), "世");

    // Deletion alone retains a selected fragment; preedit only changes display.
    for composing in [false, true] {
        system.edit(&mut editor).set_text("é你好B").unwrap();
        system.edit(&mut editor).select(selected).unwrap();
        if composing {
            system.edit(&mut editor).apply_ime(preedit).unwrap();
        }
        let edit = ImeEdit {
            commit: None,
            preedit: if composing { "日" } else { "" },
            ..commit
        };
        system.edit(&mut editor).apply_ime(edit).unwrap();
        assert_eq!(editor.text(), "你好");
        assert_eq!(editor.history_stats().undo_steps, 1);
        system.edit(&mut editor).cancel_preedit();
        assert_eq!(editor.display_text(), "你好");
        assert_eq!(editor.selection(), selection(6, 0));
        system.edit(&mut editor).undo().unwrap();
        assert_eq!(editor.text(), "é你好B");
        assert_eq!(editor.selection(), selected);
    }
    system.edit(&mut editor).apply_ime(preedit).unwrap();
    system
        .edit(&mut editor)
        .apply_ime(ImeEdit::default())
        .unwrap();
    assert_eq!(editor.text(), "éB");
    system.edit(&mut editor).undo().unwrap();
    assert_eq!(editor.text(), "é你好B");
    system.edit(&mut editor).set_read_only(true);
    assert_eq!(
        system.edit(&mut editor).apply_ime(commit),
        Err(TextError::ReadOnly)
    );
}
