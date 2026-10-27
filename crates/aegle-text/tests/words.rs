//! Word-wise movement and double-click selection, whose only boundary source is
//! the system's segmenter. Without one (`AEGLE_SEGMENT=none`, or a Linux system
//! without libicu) the word operations do nothing, which the tests assert
//! rather than skipping.

use aegle_text::{
    Alignment, Editor, EditorOptions, HitSelection, Movement, Selection, TextStyle, TextSystem,
    segment,
};
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

fn editor(system: &mut TextSystem, text: &str) -> Editor {
    let mut editor = system.editor(text, &style(), MULTILINE).unwrap();
    system
        .edit(&mut editor)
        .reflow(Some(400.0), Alignment::Left)
        .unwrap();
    editor
}

/// Moves the caret by words from the start and returns the byte offsets it
/// stops at, ending at the end of the text.
fn word_stops(system: &mut TextSystem, text: &str) -> Vec<usize> {
    let mut editor = editor(system, text);
    // A fresh editor selects the end of its text.
    system
        .edit(&mut editor)
        .move_cursor(Movement::TextStart, false)
        .unwrap();
    let mut stops = Vec::new();
    for _ in 0..16 {
        system
            .edit(&mut editor)
            .move_cursor(Movement::WordRight, false)
            .unwrap();
        let at = editor.selection().focus;
        if stops.last() == Some(&at) {
            break;
        }
        stops.push(at);
        if at == text.len() {
            break;
        }
    }
    stops
}

/// The byte offsets that are word starts per the system segmenter.
fn bounds(text: &str) -> Vec<usize> {
    segment::boundaries(text)
        .expect("a segmenter")
        .starts()
        .to_vec()
}

#[test]
fn latin_words_step_over_spaces() {
    if !segment::available() {
        eprintln!("no system segmenter here; skipping word steps");
        return;
    }
    let mut system = system();
    assert_eq!(word_stops(&mut system, "one two three"), [3, 7, 13]);
    // Backward stepping stops at word starts, forward stepping at word ends.
    assert_eq!(word_stops_ending(&mut system, "one two three"), [8, 4, 0]);
}

fn word_stops_ending(system: &mut TextSystem, text: &str) -> Vec<usize> {
    let mut editor = editor(system, text);
    system
        .edit(&mut editor)
        .move_cursor(Movement::TextEnd, false)
        .unwrap();
    let mut stops = Vec::new();
    for _ in 0..16 {
        system
            .edit(&mut editor)
            .move_cursor(Movement::WordLeft, false)
            .unwrap();
        let at = editor.selection().anchor;
        if stops.last() == Some(&at) {
            break;
        }
        stops.push(at);
    }
    stops
}

#[test]
fn chinese_words_come_from_the_system_dictionary() {
    if !segment::available() {
        eprintln!("no system segmenter here; Chinese word operations are disabled");
        return;
    }
    let text = "中文分词测试";
    let words = bounds(text);
    assert!(words.len() > 2, "expected several words, got {words:?}");
    // Word movement stops exactly where the segmenter says words begin.
    let stops = word_stops(&mut system(), text);
    assert_eq!(stops, words[1..], "stops {stops:?} vs words {words:?}");
    // And double-click selects the word under the pointer.
    let mut system = system();
    let mut editor = editor(&mut system, text);
    system
        .edit(&mut editor)
        .select(Selection {
            anchor: 0,
            focus: 0,
        })
        .unwrap();
    let caret = editor.caret_rect(1.0).unwrap().expect("a caret");
    let point = aegle_types::Point::new(
        caret.origin.x + caret.size.width / 2.0,
        caret.origin.y + caret.size.height / 2.0,
    );
    system
        .edit(&mut editor)
        .select_at(point, HitSelection::Word)
        .unwrap();
    let selection: Selection = editor.selection();
    assert_eq!(&text[selection.range()], "中文", "{selection:?}");
}

#[test]
fn without_a_segmenter_word_operations_do_nothing() {
    if segment::available() {
        eprintln!("a system segmenter is present; the disabled path is not exercised here");
        return;
    }
    let text = "one two three";
    let mut system = system();
    // Word stepping neither moves nor falls back to characters or to another
    // notion of a word.
    assert_eq!(word_stops(&mut system, text), [0]);
    assert_eq!(word_stops_ending(&mut system, text), [text.len()]);
    // A double click places the caret where it was clicked instead of
    // selecting an approximate word.
    let mut editor = editor(&mut system, "中文分词测试");
    system
        .edit(&mut editor)
        .select(Selection {
            anchor: 0,
            focus: 0,
        })
        .unwrap();
    let caret = editor.caret_rect(1.0).unwrap().expect("a caret");
    let point = aegle_types::Point::new(
        caret.origin.x + caret.size.width / 2.0,
        caret.origin.y + caret.size.height / 2.0,
    );
    system
        .edit(&mut editor)
        .select_at(point, HitSelection::Word)
        .unwrap();
    assert!(
        editor.selected_text().is_empty(),
        "{}",
        editor.selected_text()
    );
}

#[test]
fn thai_without_spaces_uses_the_dictionary() {
    if !segment::available() {
        eprintln!("no system segmenter here; Thai word operations are disabled");
        return;
    }
    let text = "ภาษาไทยทดสอบ";
    assert!(bounds(text).len() > 2);
}
