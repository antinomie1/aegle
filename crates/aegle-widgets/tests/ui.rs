//! One retained lifecycle scenario across input, callbacks, IME, themes and destruction.
use aegle_text::{Blob, GenericFamily, Selection};
use aegle_ui::{
    ClipboardRequest, ImeEdit, Key, KeyInput, Modifiers, Result, Size, TextSystem, Theme, Ui,
};
use aegle_widgets::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[test]
fn retained_controls_share_state_without_callback_borrows_or_ownership_cycles() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let fonts = Rc::new(RefCell::new(fonts));
    let ui = Ui::with_fonts(fonts.clone(), Theme::default())?;
    let root = ui.root();
    let row = root.row();
    row.set_padding(3.0);
    let label = row.text("Hello");
    let field = root.text_area("Hello, 世界\n你好 / 日本語 / 한글\nthird\nfourth\nfifth\nsixth");
    field.set_width(None);
    field.set_height(Some(60.0));
    field.set_min_width(0.0);
    field.set_min_height(0.0);
    let button = root.button("Clear");
    ui.resize(Size::new(320.0, 200.0));
    assert!(ui.refresh()?);
    assert!(!ui.refresh()?);
    #[cfg(feature = "accessibility")]
    {
        label.set_text("Changed");
        let snapshot = ui.accessibility(true, "Lifecycle")?;
        assert!(ui.refresh()?, "semantic inspection consumed pending pixels");
        assert!(!ui.refresh()?);
        let root_id = snapshot.tree.as_ref().unwrap().root;
        for (id, _) in &snapshot.nodes {
            assert!(
                *id == root_id
                    || snapshot
                        .nodes
                        .iter()
                        .any(|(_, parent)| parent.children().contains(id)),
                "unparented {id:?}"
            );
        }
    }

    assert!(label.bounds().origin.x >= Theme::default().padding + 3.0);
    field.focus();
    field.select(Selection::default());
    ui.refresh()?;
    let first = ui.take_ime_state(4000).unwrap().request.unwrap();
    let length = field.text().len();
    field.select(Selection {
        anchor: length,
        focus: length,
    });
    ui.refresh()?;
    let last = ui.take_ime_state(4000).unwrap().request.unwrap();
    assert!(last.cursor_rect.origin.y < field.bounds().origin.y + field.bounds().size.height);
    assert!(last.selection.focus > first.selection.focus);
    ui.ime(ImeEdit {
        preedit: "世界",
        cursor: Some(Selection {
            anchor: 6,
            focus: 6,
        }),
        ..Default::default()
    })?;
    ui.refresh()?;
    let committed = field.text();
    ui.set_theme(Theme::dark());
    ui.refresh()?;
    #[cfg(feature = "accessibility")]
    {
        let tree = ui.accessibility(false, "Lifecycle")?;
        let color = tree
            .nodes
            .iter()
            .find_map(|(_, n)| n.foreground_color())
            .unwrap();
        assert_eq!(
            [color.red, color.green, color.blue, color.alpha],
            Theme::dark().foreground.to_rgba()
        );
    }
    assert_eq!(field.text(), committed);
    assert_eq!(field.bounds().size.height, 60.0);
    ui.window_focus(false)?;
    ui.window_focus(true)?;
    assert!(ui.take_ime_state(4000).unwrap().request.is_some());
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    let label_copy = label.clone();
    let field_copy = field.clone();
    button.on_click(move |button| {
        count.set(count.get() + 1);
        field_copy.set_text("");
        label_copy.set_text("Done");
        button.on_click(|button| button.remove());
        button.activate()
    });
    button.activate();
    ui.dispatch_callbacks()?;
    assert_eq!(clicks.get(), 1);
    assert_eq!(label.text(), "Done");
    assert!(button.is_alive());
    assert!(ui.has_pending_callbacks());
    ui.dispatch_callbacks()?;
    assert!(!button.is_alive());
    assert!(panics(|| button.activate()));
    let moved = row.text("move");
    moved.reparent(&root);
    row.remove();
    assert!(!label.is_alive());
    assert!(moved.is_alive());
    let single = root.text_field("submit");
    let submits = Rc::new(Cell::new(0));
    let count = submits.clone();
    single.on_submit(move |_| {
        count.set(count.get() + 1);
        Ok(())
    });
    single.focus();
    ui.key(KeyInput {
        key: Key::Enter,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    })?;
    ui.dispatch_callbacks()?;
    assert_eq!(submits.get(), 1);

    // Shortcuts hand clipboard work to the host; cut deletes immediately and
    // single-line paste drops line breaks.
    let shortcut = |key: char, control: bool| KeyInput {
        key: Key::Character(key),
        text: if control { "" } else { "!" },
        modifiers: Modifiers {
            control,
            ..Modifiers::default()
        },
        pressed: true,
        repeat: false,
    };
    single.select(Selection {
        anchor: 0,
        focus: 3,
    });
    ui.key(shortcut('x', true))?;
    assert_eq!(
        ui.take_clipboard(),
        Some(ClipboardRequest::Write("sub".into()))
    );
    ui.key(shortcut('v', true))?;
    assert_eq!(ui.take_clipboard(), Some(ClipboardRequest::Read));
    ui.paste("re\nsub")?;
    assert_eq!(single.text(), "resubmit");
    // Passwords keep their value behind masks and never reach the clipboard.
    single.set_password(true);
    ui.key(shortcut('1', false))?;
    ui.key(shortcut('a', true))?;
    ui.key(shortcut('c', true))?;
    assert_eq!(ui.take_clipboard(), None);
    assert_eq!(single.text(), "resubmit!");
    ui.refresh()?;
    assert!(ui.take_ime_state(4000).unwrap().request.is_none());
    ui.refresh()?;
    ui.take_ime_state(4000);
    field.select(Selection::default());
    field.set_text("background");
    field.set_read_only(true);
    ui.refresh()?;
    assert!(ui.take_ime_state(4000).is_none_or(|state| !state.reset));

    root.set_enabled(false);
    assert!(ui.take_ime_state(4000).unwrap().request.is_none());
    root.set_enabled(true);
    ui.key(KeyInput {
        key: Key::Tab,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    })?;
    ui.refresh()?;
    #[cfg(feature = "accessibility")]
    {
        let tree = ui.accessibility(true, "Lifecycle")?;
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == aegle_access::accesskit::Role::MultilineTextInput)
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.role() == aegle_access::accesskit::Role::PasswordInput)
        );
        assert!(
            !tree
                .nodes
                .iter()
                .any(|(_, node)| node.value().is_some_and(|value| value.contains("sub")))
        );
        assert!(!ui.access_dirty());
    }
    drop(ui);
    assert!(!field.is_alive());
    assert!(panics(|| field.set_text("dead")));
    assert_eq!(Rc::strong_count(&fonts), 1);
    Ok(())
}

#[test]
fn first_activation_focuses_without_a_focus_cue_until_a_key() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::default())?;
    let first = ui.root().button("First");
    let second = ui.root().button("Second");
    ui.resize(Size::new(200.0, 120.0));
    ui.refresh()?;
    ui.window_focus(true)?;
    assert!(first.is_focused());
    assert!(!first.visual_state().focused, "no cue before keyboard use");
    ui.key(KeyInput {
        key: Key::Tab,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    })?;
    assert!(second.is_focused());
    assert!(second.visual_state().focused);
    Ok(())
}

/// Whether `change` panics, as handle methods on removed controls do.
fn panics(change: impl FnOnce()) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(change)).is_err()
}
