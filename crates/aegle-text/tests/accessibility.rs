//! The semantic snapshot and assistive selection share the retained editor.
#![cfg(feature = "text-a11y")]

use accesskit::{Node, NodeId, Role, TextPosition, TextSelection, TreeUpdate};
use aegle_text::{Blob, Editor, EditorOptions, Selection, TextError, TextStyle, TextSystem};
use aegle_types::Point;
use std::sync::Arc;

fn export(system: &mut TextSystem, editor: &mut Editor, id: &mut u64) -> (Node, TreeUpdate) {
    let mut node = Node::new(Role::Unknown);
    let mut update = TreeUpdate {
        nodes: Vec::new(),
        tree: None,
        tree_id: accesskit::TreeId::ROOT,
        focus: NodeId(1),
    };
    system
        .edit(editor)
        .accessibility(
            &mut update,
            &mut node,
            || {
                *id += 1;
                NodeId(*id)
            },
            Point::new(4.0, 8.0),
        )
        .unwrap();
    (node, update)
}

#[test]
fn selection_round_trip_obeys_run_boundaries_readonly_and_composition() {
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
        .editor(
            "你好\nAe\u{301}B",
            &style,
            EditorOptions {
                multiline: true,
                ..Default::default()
            },
        )
        .unwrap();
    let mut id = 1;
    let whole = Selection {
        anchor: editor.display_text().len(),
        focus: 0,
    };
    system.edit(&mut editor).select(whole).unwrap();
    let (node, update) = export(&mut system, &mut editor, &mut id);
    assert_eq!(node.role(), Role::MultilineTextInput);
    let text: String = update.nodes.iter().filter_map(|(_, n)| n.value()).collect();
    assert_eq!(text, editor.display_text());
    let saved = node.text_selection().unwrap().clone();
    system
        .edit(&mut editor)
        .select(Selection::default())
        .unwrap();
    system
        .edit(&mut editor)
        .select_accessibility(&saved)
        .unwrap();
    assert_eq!(editor.selection(), whole);
    let (first_id, first) = &update.nodes[0];
    let selection = TextSelection {
        anchor: TextPosition {
            node: *first_id,
            character_index: first.character_lengths().len(),
        },
        focus: TextPosition {
            node: *first_id,
            character_index: 0,
        },
    };
    system.edit(&mut editor).set_read_only(true);
    system
        .edit(&mut editor)
        .select_accessibility(&selection)
        .unwrap();
    assert_eq!(editor.selected_text(), first.value().unwrap());
    let mut invalid = selection.clone();
    invalid.anchor.character_index = usize::MAX;
    editor.take_changes();
    assert_eq!(
        system.edit(&mut editor).select_accessibility(&invalid),
        Err(TextError::InvalidRange)
    );
    assert_eq!(editor.take_changes(), Default::default());
    let (node, _) = export(&mut system, &mut editor, &mut id);
    assert!(node.is_read_only());
    system.edit(&mut editor).set_read_only(false);
    system.edit(&mut editor).set_preedit("中文", None).unwrap();
    let (node, update) = export(&mut system, &mut editor, &mut id);
    assert!(node.text_selection().is_none());
    assert!(update.nodes.iter().any(|(_, n)| n.underline().is_some()));
    assert_eq!(editor.text(), "你好\nAe\u{301}B");
    assert_eq!(
        system.edit(&mut editor).select_accessibility(&selection),
        Err(TextError::CompositionActive)
    );
    system.edit(&mut editor).cancel_preedit();
    assert_eq!(
        system.edit(&mut editor).select_accessibility(&selection),
        Err(TextError::InvalidRange)
    );
    system.edit(&mut editor).set_text("").unwrap();
    let (node, _) = export(&mut system, &mut editor, &mut id);
    system
        .edit(&mut editor)
        .select_accessibility(node.text_selection().unwrap())
        .unwrap();
    assert_eq!(editor.selection(), Selection::default());
}
