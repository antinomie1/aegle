//! The input-method text model: `NSTextInputClient`-style operations and the
//! `ImeUpdate`s they become, without any AppKit object.
//!
//! Ranges the system uses count UTF-16 units; `ImeUpdate` offsets count UTF-8
//! bytes. Every case here pins that boundary, because an off-by-one silently
//! edits the wrong text.
use aegle_platform::ImeUpdate;
use aegle_platform_macos::Composition;

#[test]
fn utf16_queries_address_the_document() {
    // "ab你" is three UTF-16 units but five UTF-8 bytes.
    let mut composition = Composition::new();
    assert!(composition.focus("ab你好cd", 5, 5));
    assert!(composition.is_focused());
    assert_eq!(composition.selection(), 5..5);
    assert_eq!(composition.selected_range(), 3..3);
    assert_eq!(composition.marked_range(), None);
    assert_eq!(composition.document(), "ab你好cd");
    let (text, actual) = composition.substring(0..3).unwrap();
    assert_eq!((text.as_str(), actual), ("ab你", 0..3));
    // A surrogate pair is two UTF-16 units and four UTF-8 bytes.
    let mut emoji = Composition::new();
    emoji.focus("a😀b", 5, 5);
    assert_eq!(emoji.selected_range(), 3..3);
    assert_eq!(emoji.substring(0..3).unwrap().0, "a😀");
}

#[test]
fn a_composition_stands_in_for_the_selection() {
    let mut composition = Composition::new();
    composition.focus("ab你好cd", 5, 5);
    // Starting a composition replaces the selection, which is empty here.
    let update = composition.set_marked("に", 1..1, None).unwrap();
    assert_eq!(
        update,
        ImeUpdate {
            delete_before: 0,
            delete_after: 0,
            commit: Some(String::new()),
            preedit: aegle_platform::Preedit {
                text: "に".into(),
                cursor: Some((3, 3)),
            },
        }
    );
    assert!(composition.is_composing());
    assert_eq!(composition.document(), "ab你に好cd");
    assert_eq!(composition.marked_range(), Some(3..4));
    assert_eq!(composition.selected_range(), 4..4);
    // A further marking replaces the composition instead of the selection.
    let update = composition.set_marked("にほ", 2..2, None).unwrap();
    assert_eq!(update.commit, None);
    assert_eq!(update.preedit.text, "にほ");
    assert_eq!(update.preedit.cursor, Some((6, 6)));
    assert_eq!(composition.marked_range(), Some(3..5));
    // Committing ends the composition with the text as one edit.
    let update = composition.insert("日本", None).unwrap();
    assert_eq!(
        update,
        ImeUpdate {
            delete_before: 0,
            delete_after: 0,
            commit: Some("日本".into()),
            preedit: Default::default(),
        }
    );
    assert!(!composition.is_composing());
    assert_eq!(composition.surrounding(), "ab你日本好cd");
    assert_eq!(composition.selection(), 11..11);
}

#[test]
fn a_replacement_wider_than_the_selection_deletes_around_it() {
    let mut composition = Composition::new();
    // The host selects "ello" and the input method replaces "hello".
    composition.focus("hello", 5, 1);
    let update = composition.insert("x", Some(0..5)).unwrap();
    assert_eq!((update.delete_before, update.delete_after), (1, 0));
    assert_eq!(update.commit.as_deref(), Some("x"));
    assert_eq!(composition.surrounding(), "x");
    assert_eq!(composition.selection(), 1..1);
    // Replacing exactly the selection deletes nothing around it.
    let mut exact = Composition::new();
    exact.focus("hello", 5, 1);
    let update = exact.insert("i", Some(1..5)).unwrap();
    assert_eq!((update.delete_before, update.delete_after), (0, 0));
    // A range that does not enclose the selection, or one beyond the
    // document, cannot be expressed and changes only the selection.
    let mut outside = Composition::new();
    outside.focus("hello", 5, 1);
    let update = outside.insert("x", Some(0..3)).unwrap();
    assert_eq!((update.delete_before, update.delete_after), (0, 0));
    let update = outside.insert("x", Some(100..200)).unwrap();
    assert_eq!((update.delete_before, update.delete_after), (0, 0));
}

#[test]
fn typed_text_stays_with_the_key() {
    let mut composition = Composition::new();
    composition.focus("", 0, 0);
    composition.key_down();
    // Text typed outside a composition is the key's own text, not an edit.
    assert!(composition.insert("a", None).is_none());
    let outcome = composition.key_up();
    assert!(!outcome.consumed);
    assert_eq!(outcome.text.as_deref(), Some("a"));
    assert_eq!(composition.surrounding(), "");
    // While composing, the same call is a commit and consumes the key.
    composition.set_marked("に", 0..0, None);
    composition.key_down();
    let update = composition.insert("日本", None).unwrap();
    assert_eq!(update.commit.as_deref(), Some("日本"));
    let outcome = composition.key_up();
    assert!(outcome.consumed);
    assert_eq!(outcome.text, None);
    // Marking text during a key press also consumes it.
    composition.focus("", 0, 0);
    composition.key_down();
    assert!(composition.set_marked("に", 1..1, None).is_some());
    assert!(composition.key_up().consumed);
}

#[test]
fn unmark_commits_the_composition_and_blur_discards_it() {
    let mut composition = Composition::new();
    composition.focus("ab", 2, 2);
    composition.set_marked("にほ", 2..2, None);
    let update = composition.unmark().unwrap();
    assert_eq!(update.commit.as_deref(), Some("にほ"));
    assert_eq!(update.preedit.text, "");
    assert_eq!(composition.surrounding(), "abにほ");
    assert!(!composition.is_composing());
    // Unmarking without a composition reports nothing.
    assert!(composition.unmark().is_none());

    // Focus leaving drops the composition without committing it again.
    let mut composition = Composition::new();
    composition.focus("ab", 2, 2);
    composition.set_marked("に", 1..1, None);
    assert!(composition.blur());
    assert!(!composition.is_composing());
    assert!(!composition.is_focused());
    assert!(composition.insert("x", None).is_none());
    assert!(composition.set_marked("y", 0..0, None).is_none());
    // A new editor gets a session of its own.
    assert!(composition.focus("cd", 2, 2));
    assert_eq!(composition.surrounding(), "cd");
    assert!(composition.marked_range().is_none());
}
