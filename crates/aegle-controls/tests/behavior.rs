//! Shared control lifecycle across native and semantic input.

use aegle_controls::{
    Action, Button, Capture, Input, Key, KeyInput, Modifiers, PointerId, PointerInput, PointerKind,
};
use aegle_types::Point;

fn pointer(id: u64, kind: PointerKind, inside: bool) -> Input<'static> {
    Input::Pointer(PointerInput {
        id: PointerId(id),
        kind,
        inside,
        position: Point::default(),
        modifiers: Modifiers::default(),
    })
}

fn key(key: Key, pressed: bool, repeat: bool) -> Input<'static> {
    Input::Key(KeyInput {
        key,
        text: "",
        pressed,
        repeat,
        modifiers: Modifiers::default(),
    })
}

#[test]
fn capture_keyboard_and_semantic_activation_share_lifecycle() {
    let mut button = Button::new();
    let press = button.handle(pointer(1, PointerKind::Down { clicks: 1 }, true));
    assert_eq!(press.capture, Some(Capture::Acquire(PointerId(1))));
    assert!(press.focus && button.is_pressed());
    assert_eq!(
        button.handle(pointer(2, PointerKind::Up, true)).action,
        None
    );
    button.handle(pointer(1, PointerKind::Move, false));
    assert!(!button.is_pressed());
    assert_eq!(
        button.handle(pointer(1, PointerKind::Up, false)).action,
        None
    );
    button.handle(pointer(1, PointerKind::Down { clicks: 1 }, true));
    assert_eq!(
        button.set_enabled(false).capture,
        Some(Capture::Release(PointerId(1)))
    );
    assert_eq!(button.handle(Input::Activate).action, None);
    assert_eq!(
        button.handle(pointer(1, PointerKind::Up, true)).action,
        None
    );
    button.set_enabled(true);
    button.handle(Input::Focus(true));
    assert_eq!(
        button.handle(key(Key::Character(' '), true, false)).action,
        None
    );
    assert_eq!(
        button.handle(key(Key::Character(' '), true, true)).action,
        None
    );
    assert_eq!(
        button.handle(key(Key::Character(' '), false, false)).action,
        Some(Action::Activate)
    );
    assert_eq!(
        button.handle(key(Key::Enter, true, false)).action,
        Some(Action::Activate)
    );
    assert_eq!(button.handle(key(Key::Enter, true, true)).action, None);
    button.handle(pointer(1, PointerKind::Move, true));
    button.handle(Input::Focus(false));
    assert!(!button.is_pressed() && button.is_hovered());
    assert_eq!(button.handle(key(Key::Enter, false, false)).action, None);
    assert_eq!(
        button.handle(Input::Activate).action,
        Some(Action::Activate)
    );
}

#[test]
#[cfg(feature = "text")]
fn field_keeps_editor_state_across_pointer_ime_focus_and_read_only_changes() {
    use aegle_controls::TextField;
    use aegle_text::{Blob, EditorOptions, ImeEdit, Selection, TextStyle, TextSystem};
    use std::sync::Arc;
    let mut fonts = TextSystem::new();
    fonts
        .register_fonts(Blob::new(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        )))
        .unwrap();
    let editor = fonts
        .editor(
            "你好",
            &TextStyle {
                families: "Aegle Test CJK",
                ..Default::default()
            },
            EditorOptions::default(),
        )
        .unwrap();
    let mut field = TextField::new(editor);
    field.handle(&mut fonts, Input::Focus(true)).unwrap();
    fonts
        .edit(field.editor_mut())
        .select(Selection {
            anchor: 0,
            focus: 6,
        })
        .unwrap();
    field
        .handle(
            &mut fonts,
            Input::Ime(ImeEdit {
                preedit: "世界",
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(field.editor().text(), "你好");
    assert!(
        field
            .handle(&mut fonts, Input::Focus(false))
            .unwrap()
            .reset_ime
    );
    assert_eq!(field.editor().display_text(), "你好");
    field.handle(&mut fonts, Input::Focus(true)).unwrap();
    field
        .handle(
            &mut fonts,
            Input::Ime(ImeEdit {
                commit: Some("世界"),
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(field.editor().text(), "世界");
    assert_eq!(
        field
            .handle(&mut fonts, key(Key::Enter, true, false))
            .unwrap()
            .action,
        Some(Action::Submit)
    );
    assert_eq!(
        field
            .handle(&mut fonts, key(Key::Enter, true, true))
            .unwrap()
            .action,
        None
    );
    let down = field
        .handle(
            &mut fonts,
            pointer(1, PointerKind::Down { clicks: 1 }, true),
        )
        .unwrap();
    assert_eq!(down.capture, Some(Capture::Acquire(PointerId(1))));
    let disabled = field.set_enabled(&mut fonts, false);
    assert_eq!(disabled.capture, Some(Capture::Release(PointerId(1))));
    assert!(disabled.reset_ime);
    field.set_enabled(&mut fonts, true);
    fonts.edit(field.editor_mut()).set_read_only(true);
    assert!(!field.accepts_ime());
    field
        .handle(&mut fonts, key(Key::Backspace, true, false))
        .unwrap();
    assert_eq!(field.editor().text(), "世界");
}
