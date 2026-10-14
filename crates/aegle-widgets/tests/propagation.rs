//! Input goes down through capture listeners to its target and back up
//! through bubbling ones before the control handles it; listeners may stop
//! the path or prevent the default, and every handler can be removed, even
//! while its event is being dispatched.
use std::{cell::RefCell, rc::Rc};

use aegle_ui::{
    InputKind, Key, KeyInput, Modifiers, Node, Phase, Point, PointerId, PointerKind, Result, Size,
    TextSystem, Theme, Ui,
};
use aegle_widgets::Widgets;

type Log = Rc<RefCell<Vec<(&'static str, Phase)>>>;

fn click(ui: &Ui, node: &Node) -> Result {
    let bounds = node.bounds();
    let at = Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    let pointer = |kind| ui.pointer(PointerId(1), kind, at, Modifiers::default());
    pointer(PointerKind::Down { clicks: 1 })?;
    pointer(PointerKind::Up)?;
    ui.dispatch_callbacks()
}

/// Records pointer presses reaching `node` under `name`.
fn listen(node: &Node, log: &Log, name: &'static str, capture: bool) -> aegle_ui::Subscription {
    let log = log.clone();
    let record = move |_: Node, event: &mut aegle_ui::InputEvent<'_>| {
        if let InputKind::Pointer {
            kind: PointerKind::Down { .. },
            ..
        } = event.kind
        {
            log.borrow_mut().push((name, event.phase()));
        }
    };
    match capture {
        true => node.on_input_capture(record),
        false => node.on_input(record),
    }
}

#[test]
fn input_captures_down_to_its_target_and_bubbles_back() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 200.0));
    let panel = ui.root().column();
    let button = panel.button("");
    let clicks = Rc::new(RefCell::new(0));
    let count = clicks.clone();
    button.on_click(move |_| *count.borrow_mut() += 1);
    let log: Log = Default::default();
    let down = listen(&panel, &log, "panel", true);
    listen(&panel, &log, "panel", false);
    listen(&button, &log, "button", false);
    ui.refresh()?;
    click(&ui, &button)?;
    assert_eq!(
        *log.borrow(),
        [
            ("panel", Phase::Capture),
            ("button", Phase::Target),
            ("panel", Phase::Bubble)
        ]
    );
    assert_eq!(*clicks.borrow(), 1);

    // Stopping at the target skips the bubbling ancestor, not the click.
    let stop = button.on_input(|_, event| event.stop_propagation());
    log.borrow_mut().clear();
    click(&ui, &button)?;
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(*clicks.borrow(), 2);
    stop.remove();

    // Preventing the default in capture keeps the button from being pressed.
    let prevent = panel.on_input_capture(|_, event| event.prevent_default());
    down.remove();
    log.borrow_mut().clear();
    click(&ui, &button)?;
    assert_eq!(log.borrow()[0], ("button", Phase::Target));
    assert_eq!(*clicks.borrow(), 2);
    prevent.remove();
    click(&ui, &button)?;
    assert_eq!(*clicks.borrow(), 3);

    // Keys travel the focused control's path; preventing skips the control.
    button.focus();
    let keys = Rc::new(RefCell::new(Vec::new()));
    let seen = keys.clone();
    let prevent = ui.root().on_input_capture(move |_, event| {
        if let InputKind::Key(key) = event.kind {
            seen.borrow_mut().push(key.key);
            event.prevent_default();
        }
    });
    let space = |pressed| KeyInput {
        key: Key::Character(' '),
        text: " ",
        modifiers: Modifiers::default(),
        pressed,
        repeat: false,
    };
    ui.key(space(true))?;
    ui.key(space(false))?;
    ui.dispatch_callbacks()?;
    assert_eq!(keys.borrow().len(), 2);
    assert_eq!(*clicks.borrow(), 3);
    prevent.remove();
    ui.key(space(true))?;
    ui.key(space(false))?;
    ui.dispatch_callbacks()?;
    assert_eq!(*clicks.borrow(), 4);
    Ok(())
}

#[test]
fn handlers_are_removed_even_mid_dispatch() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 200.0));
    let button = ui.root().button("");
    let log = Rc::new(RefCell::new(Vec::new()));
    let second: Rc<RefCell<Option<aegle_ui::Subscription>>> = Default::default();
    let (first_log, pending) = (log.clone(), second.clone());
    // The first handler removes the second before it runs, and itself.
    let first: Rc<RefCell<Option<aegle_ui::Subscription>>> = Default::default();
    let this = first.clone();
    *first.borrow_mut() = Some(button.on_click(move |_| {
        first_log.borrow_mut().push("first");
        if let Some(second) = pending.borrow_mut().take() {
            second.remove();
        }
        if let Some(this) = this.borrow_mut().take() {
            this.remove();
        }
    }));
    let second_log = log.clone();
    *second.borrow_mut() = Some(button.on_click(move |_| second_log.borrow_mut().push("second")));
    let third_log = log.clone();
    button.on_click(move |_| third_log.borrow_mut().push("third"));
    button.activate();
    ui.dispatch_callbacks()?;
    assert_eq!(*log.borrow(), ["first", "third"]);
    button.activate();
    ui.dispatch_callbacks()?;
    assert_eq!(*log.borrow(), ["first", "third", "third"]);

    // Window key and frame handlers unsubscribe the same way.
    let keys = Rc::new(RefCell::new(0));
    let count = keys.clone();
    let key = ui.on_key(move |_| {
        *count.borrow_mut() += 1;
        false
    });
    let enter = KeyInput {
        key: Key::Enter,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    };
    ui.key(enter)?;
    key.remove();
    key.remove();
    ui.key(enter)?;
    assert_eq!(*keys.borrow(), 1);
    let frame = button.on_frame(|_, _| true);
    assert!(ui.wants_frames());
    frame.remove();
    assert!(!ui.wants_frames());
    // A removed control's subscriptions do nothing.
    let late = button.on_click(|_| ());
    button.remove();
    late.remove();
    Ok(())
}
