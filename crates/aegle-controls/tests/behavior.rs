//! Shared control lifecycle across native and semantic input.

use aegle_controls::{
    Action, Button, Capture, Input, Key, KeyInput, Modifiers, PointerId, PointerInput, PointerKind,
    Range, Slider, Toggle,
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
fn toggles_and_ranges_share_changes_capture_and_numeric_boundaries() {
    let mut toggle = Toggle::new(false);
    assert!(toggle.set_checked(true).semantics);
    assert_eq!(toggle.set_checked(true).action, None);
    assert_eq!(toggle.handle(Input::Activate).action, Some(Action::Change));
    assert!(!toggle.is_checked());
    toggle.handle(Input::Focus(true));
    toggle.handle(key(Key::Character(' '), true, false));
    assert_eq!(
        toggle.handle(key(Key::Character(' '), true, true)).action,
        None
    );
    assert_eq!(
        toggle.handle(key(Key::Character(' '), false, false)).action,
        Some(Action::Change)
    );
    toggle.set_enabled(false);
    assert_eq!(toggle.handle(Input::Activate).action, None);

    assert!(Range::new(-f64::MAX, f64::MAX, 0.0, 0.0).is_err());
    let tiny = f64::from_bits(1);
    let range = Range::new(-f64::MAX / 2.0, f64::MAX / 2.0, 1.0, tiny).unwrap();
    assert_eq!(range.value(), 1.0);
    assert_eq!(
        Range::new(0.0, tiny * 2.0, tiny, tiny).unwrap().fraction(),
        0.5
    );
    for step in [0.0, 0.1, 3.0, 20.0] {
        let mut range = Range::new(-2.0, 10.0, 0.0, step).unwrap();
        for index in 0..=100 {
            range.set_value(f64::from(index) / 10.0).unwrap();
            assert!(
                !range.set_value(range.value()).unwrap(),
                "unstable {range:?}"
            );
        }
    }
    let mut range = Range::new(0.0, 10.0, 1.5, 3.0).unwrap();
    assert_eq!(range.value(), 3.0);
    let before = range;
    assert!(range.set_value(f64::NAN).is_err());
    assert!(range.set_bounds(10.0, 0.0).is_err());
    assert!(range.set_step(-1.0).is_err());
    assert_eq!(range, before);
    assert!(range.set_step(0.0).unwrap());
    assert!(range.set_bounds(0.0, 2.0).unwrap());
    assert_eq!(range.value(), 2.0);

    let at = |id, kind, x, inside| {
        Input::Pointer(PointerInput {
            id: PointerId(id),
            kind,
            position: Point::new(x, 0.0),
            inside,
            modifiers: Modifiers::default(),
        })
    };
    let down = PointerKind::Down { clicks: 1 };
    let mut slider = Slider::new(Range::new(0.0, 10.0, 0.0, 3.0).unwrap());
    assert!(slider.handle(at(1, down, f32::NAN, true), 100.0).is_err());
    assert!(!slider.is_pressed());
    let started = slider.handle(at(1, down, 30.0, true), 100.0).unwrap();
    assert_eq!(started.capture, Some(Capture::Acquire(PointerId(1))));
    assert_eq!(started.action, Some(Action::Change));
    assert_eq!(
        slider
            .handle(at(2, PointerKind::Up, 90.0, true), 100.0)
            .unwrap()
            .action,
        None
    );
    assert_eq!(slider.range().value(), 3.0);
    slider
        .handle(at(1, PointerKind::Move, 200.0, false), 100.0)
        .unwrap();
    assert_eq!(slider.range().value(), 10.0);
    assert!(slider.is_pressed());
    assert!(!slider.is_hovered());
    assert!(!slider.update_hover(PointerId(2), true).repaint);
    let hover = slider.update_hover(PointerId(1), true);
    assert!(hover.repaint && slider.is_hovered() && slider.is_pressed());
    assert_eq!((hover.action, hover.capture), (None, None));
    assert_eq!(slider.range().value(), 10.0);
    let up = slider
        .handle(at(1, PointerKind::Up, -20.0, false), 100.0)
        .unwrap();
    assert_eq!(up.capture, Some(Capture::Release(PointerId(1))));
    assert_eq!(slider.range().value(), 0.0);
    slider.handle(at(1, down, 60.0, true), 100.0).unwrap();
    assert_eq!(
        slider.set_enabled(false).capture,
        Some(Capture::Release(PointerId(1)))
    );
    assert_eq!(slider.handle(Input::Increment, 100.0).unwrap().action, None);
    assert!(slider.handle(Input::SetValue(f64::NAN), 100.0).is_err());
    assert_eq!(slider.range().value(), 6.0);
    slider.set_enabled(true);
    for expected in [9.0, 10.0, 10.0] {
        let old = slider.range().value();
        let changed = slider.handle(Input::Increment, 100.0).unwrap().action;
        assert_eq!(changed, (old != expected).then_some(Action::Change));
        assert_eq!(slider.range().value(), expected);
    }
    for expected in [9.0, 6.0] {
        slider.handle(Input::Decrement, 100.0).unwrap();
        assert_eq!(slider.range().value(), expected);
    }
    slider.handle(Input::Focus(true), 100.0).unwrap();
    for (key_code, value) in [
        (Key::PageDown, 0.0),
        (Key::PageUp, 10.0),
        (Key::Home, 0.0),
        (Key::End, 10.0),
    ] {
        slider.handle(key(key_code, true, false), 0.0).unwrap();
        assert_eq!(slider.range().value(), value);
    }
    slider.handle(at(1, down, 0.0, true), 0.0).unwrap();
    assert_eq!(slider.range().value(), 10.0);
    assert_eq!(
        slider.handle(Input::Focus(false), 0.0).unwrap().capture,
        Some(Capture::Release(PointerId(1)))
    );
    assert!(!slider.is_pressed());
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
