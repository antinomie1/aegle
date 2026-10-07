//! Code that runs while the UI is being changed (a control's paint, a key
//! hook) gets errors back instead of panics, and a control's self-description
//! is checked where it joins the tree.
use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use aegle_ui::{
    Control, ControlKind, Hooks, Key, KeyInput, Modifiers, Node, Result, Size, State, TextSystem,
    Theme, Ui, UiError, control::PaintCx,
};

/// Calls back into its UI from paint and counts the reentrancy errors.
struct Probe {
    ui: Weak<Ui>,
    node: Rc<RefCell<Option<Node>>>,
    refused: Rc<Cell<u32>>,
    kind: ControlKind,
}

impl Control for Probe {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        self.kind
    }
    fn paint(&mut self, _: &mut PaintCx<'_>) -> Result {
        let ui = self.ui.upgrade().unwrap();
        let node = self.node.borrow().clone().unwrap();
        for result in [
            ui.theme().map(drop),
            ui.background().map(drop),
            ui.has_pending_callbacks().map(drop),
            node.is_alive().map(drop),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.downcast_ref(), Some(&UiError::ReentrantAccess));
            self.refused.set(self.refused.get() + 1);
        }
        Ok(())
    }
}

fn refuse(_: &mut State, _: &KeyInput<'_>) -> Result<bool> {
    Err("hook failed".into())
}

static FAILING: Hooks = Hooks {
    key: Some(refuse),
    press: None,
    overlay_at: None,
    place: None,
    removed: None,
    removed_after: None,
    measure: None,
    realize: None,
    hover: None,
    wake: None,
};

#[test]
fn reentrant_calls_and_hook_failures_are_errors() -> Result {
    let ui = Rc::new(Ui::with_fonts(
        Rc::new(RefCell::new(TextSystem::new())),
        Theme::light(),
    )?);
    ui.resize(Size::new(100.0, 100.0))?;
    let (node, refused) = (Rc::new(RefCell::new(None)), Rc::new(Cell::new(0)));
    let probe = |kind| Probe {
        ui: Rc::downgrade(&ui),
        node: node.clone(),
        refused: refused.clone(),
        kind,
    };
    let added = ui.root().add(|_, theme| {
        Ok((
            Box::new(probe(ControlKind::Container)),
            aegle_ui::container_style(theme, false),
        ))
    })?;
    *node.borrow_mut() = Some(added);
    ui.refresh()?;
    assert_eq!(refused.get(), 4);

    // A text field kind without an editor would be styled as one but take no text.
    let Err(error) = ui.root().add(|_, theme| {
        Ok((
            Box::new(probe(ControlKind::TextField)),
            aegle_ui::container_style(theme, false),
        ))
    }) else {
        panic!("a text field kind without an editor was added");
    };
    assert_eq!(error.downcast_ref(), Some(&UiError::WrongKind));

    ui.state.borrow_mut().install(&FAILING);
    let key = KeyInput {
        key: Key::Enter,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    };
    assert_eq!(ui.key(key).unwrap_err().to_string(), "hook failed");
    Ok(())
}
