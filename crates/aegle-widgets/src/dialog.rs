//! In-window modal dialogs with backdrop scrim and focus management.

use aegle_controls::{Key, KeyInput};
use aegle_core::NodeId;
use aegle_layout::{
    Align, Dimension, Display, Edges, Justify, LengthPercentage, LengthPercentageAuto, Position,
};
use aegle_scene::RoundedRect;
use aegle_theme::ControlKind;
use aegle_types::{Color, Point, Rect};
use aegle_ui::{
    Container, Control, HandlerResult, Node, Result, State, container_style, control::PaintCx,
    handle,
};

use crate::{Button, Label, Widgets};

/// An entry describing one active or inactive dialog in the window.
pub(crate) struct DialogEntry {
    pub dialog: NodeId,
    pub card: NodeId,
    pub body: NodeId,
    pub footer: NodeId,
    pub title_label: NodeId,
    pub shown: bool,
    pub dismissible: bool,
    pub restore_focus: Option<NodeId>,
}

/// Storage for dialogs in one UI window.
#[derive(Default)]
pub(crate) struct Dialogs {
    pub entries: Vec<DialogEntry>,
}

pub(crate) fn dialogs(state: &mut State) -> &mut Dialogs {
    state.ext()
}

/// A control that renders the dark semi-transparent scrim behind modal dialogs.
pub struct ScrimControl;

impl Control for ScrimControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        cx.builder.fill(
            RoundedRect::new(Rect::new(0.0, 0.0, cx.size.width, cx.size.height), 0.0),
            Color::rgba(0, 0, 0, 115),
        );
    }
}

handle! {
    /// A modal dialog that floats above the window with a dimmed backdrop scrim.
    pub Dialog
}

impl Dialog {
    /// Shows the modal dialog and traps focus within its card.
    pub fn show(&self) {
        self.0.change(|state, id| {
            let focus = state.focus.current(&state.tree);
            {
                let entries = &mut dialogs(state).entries;
                let Some(entry) = entries.iter_mut().find(|e| e.dialog == id) else {
                    return Ok(());
                };
                entry.shown = true;
                entry.restore_focus = focus;
            }
            state.set_visible(id, true)?;
            Ok(())
        });
    }

    /// Closes the dialog, hides the backdrop and restores previous focus.
    pub fn close(&self) {
        self.0.change(|state, id| {
            let restore = {
                let entries = &mut dialogs(state).entries;
                let Some(entry) = entries.iter_mut().find(|e| e.dialog == id) else {
                    return Ok(());
                };
                entry.shown = false;
                entry.restore_focus
            };
            state.set_visible(id, false)?;
            if let Some(prev) = restore {
                let _ = state.set_focus(Some(prev));
            }
            state.queue_action(id);
            Ok(())
        });
    }

    /// Returns whether this dialog is currently displayed.
    pub fn is_open(&self) -> bool {
        self.0.change(|state, id| {
            Ok(dialogs(state)
                .entries
                .iter()
                .find(|e| e.dialog == id)
                .map(|e| e.shown)
                .unwrap_or(false))
        })
    }

    /// Replaces the title text displayed in the dialog header.
    pub fn set_title(&self, title: &str) {
        let label_id = self.0.change(|state, id| {
            Ok(dialogs(state)
                .entries
                .iter()
                .find(|e| e.dialog == id)
                .map(|e| e.title_label))
        });
        if let Some(label_id) = label_id {
            let label = Label(Node {
                state: self.0.state.clone(),
                id: label_id,
            });
            label.set_text(title);
        }
    }

    /// Configures whether clicking the backdrop or pressing Escape closes the dialog.
    pub fn set_dismissible(&self, dismissible: bool) {
        self.0.change(|state, id| {
            if let Some(entry) = dialogs(state).entries.iter_mut().find(|e| e.dialog == id) {
                entry.dismissible = dismissible;
            }
            Ok(())
        });
    }

    /// Returns the body container for dialog content.
    pub fn body(&self) -> Container {
        let body_id = self.0.change(|state, id| {
            Ok(dialogs(state)
                .entries
                .iter()
                .find(|e| e.dialog == id)
                .map(|e| e.body)
                .unwrap())
        });
        Container(Node {
            state: self.0.state.clone(),
            id: body_id,
        })
    }

    /// Returns the footer container for action buttons.
    pub fn footer(&self) -> Container {
        let footer_id = self.0.change(|state, id| {
            Ok(dialogs(state)
                .entries
                .iter()
                .find(|e| e.dialog == id)
                .map(|e| e.footer)
                .unwrap())
        });
        Container(Node {
            state: self.0.state.clone(),
            id: footer_id,
        })
    }

    /// Adds an action button to the dialog footer.
    pub fn button(&self, text: &str) -> Button {
        self.footer().button(text)
    }

    /// Adds a handler invoked when the dialog closes.
    pub fn on_close<R: HandlerResult>(
        &self,
        mut callback: impl FnMut() -> R + 'static,
    ) -> aegle_ui::Subscription {
        self.on_action(move |_| callback().into_result())
    }
}

/// Dismisses top open dialog when clicking outside its card.
pub(crate) fn dismiss_dialog(state: &mut State, position: Point) -> Result<bool> {
    let (card, dialog_id, dismissible) = {
        let entries = &dialogs(state).entries;
        let Some(entry) = entries.iter().rfind(|e| e.shown) else {
            return Ok(false);
        };
        (entry.card, entry.dialog, entry.dismissible)
    };

    let inside = state
        .tree
        .get(card)
        .is_some_and(|n| n.context.bounds.contains(position));

    if !inside {
        if dismissible {
            close_dialog_by_id(state, dialog_id)?;
        }
        // Absorbs click so background controls are not pressed.
        return Ok(true);
    }

    Ok(false)
}

/// Closes dialog by NodeId.
fn close_dialog_by_id(state: &mut State, id: NodeId) -> Result {
    let restore = {
        let entries = &mut dialogs(state).entries;
        let Some(entry) = entries.iter_mut().find(|e| e.dialog == id) else {
            return Ok(());
        };
        entry.shown = false;
        entry.restore_focus
    };
    state.set_visible(id, false)?;
    if let Some(prev) = restore {
        let _ = state.set_focus(Some(prev));
    }
    state.queue_action(id);
    Ok(())
}

/// Handles Escape to close dialogs.
pub(crate) fn dialog_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    if key.key != Key::Escape {
        return Ok(false);
    }
    let entries = &dialogs(state).entries;
    let Some(entry) = entries.iter().rfind(|e| e.shown && e.dismissible) else {
        return Ok(false);
    };
    let dialog_id = entry.dialog;
    close_dialog_by_id(state, dialog_id)?;
    Ok(true)
}

/// Cleans up dialog on node removal.
pub(crate) fn removed(state: &mut State, node: NodeId) {
    dialogs(state)
        .entries
        .retain(|e| e.dialog != node && e.card != node);
}

/// Constructs a modal [`Dialog`] under `container`.
pub(crate) fn create_dialog(container: &Container, title: &str) -> Dialog {
    // 1. Full-window overlay with dark backdrop scrim.
    let overlay_node = crate::add(container, |_state, theme| {
        let mut overlay_style = container_style(theme, false);
        overlay_style.position = Position::Absolute;
        overlay_style.inset = Edges {
            left: LengthPercentageAuto::length(0.0),
            top: LengthPercentageAuto::length(0.0),
            right: LengthPercentageAuto::length(0.0),
            bottom: LengthPercentageAuto::length(0.0),
        };
        overlay_style.display = Display::Flex;
        overlay_style.size = aegle_layout::Size {
            width: Dimension::percent(1.0),
            height: Dimension::percent(1.0),
        };
        overlay_style.gap = aegle_layout::Size {
            width: LengthPercentage::length(0.0),
            height: LengthPercentage::length(0.0),
        };
        overlay_style.padding = Edges {
            left: LengthPercentage::length(0.0),
            right: LengthPercentage::length(0.0),
            top: LengthPercentage::length(0.0),
            bottom: LengthPercentage::length(0.0),
        };
        overlay_style.align_items = Some(Align::Center.items());
        overlay_style.justify_content = Some(Justify::Center.content());
        Ok((Box::new(ScrimControl) as Box<dyn Control>, overlay_style))
    });
    overlay_node.set_visible(false);

    let overlay_container = Container(overlay_node.clone());

    // 2. Centered dialog card container.
    let card_group = crate::group::Group {
        role: crate::group::Role::Dialog,
    };
    let card_node = crate::add(&overlay_container, |_state, theme| {
        let mut card_style = container_style(theme, false);
        card_style.min_size.width = LengthPercentageAuto::length(320.0);
        card_style.max_size.width = LengthPercentageAuto::length(560.0);
        let p = LengthPercentage::length(theme.padding * 1.5);
        card_style.padding = Edges {
            left: p,
            right: p,
            top: p,
            bottom: p,
        };
        card_style.gap = aegle_layout::Size {
            width: LengthPercentage::length(0.0),
            height: LengthPercentage::length(theme.gap * 1.25),
        };
        Ok((Box::new(card_group) as Box<dyn Control>, card_style))
    });

    let card_container = Container(card_node.clone());

    // 3. Header row: title and close button.
    let header_row = card_container.row();
    let title_label = header_row.text(title);
    title_label.set_grow(1.0);

    let close_btn = header_row.button("✕");
    let overlay_ref = overlay_node.clone();
    close_btn.on_click(move |_| {
        let d = Dialog(overlay_ref.clone());
        d.close();
    });

    // 4. Body column.
    let body_col = card_container.column();
    body_col.set_grow(1.0);

    // 5. Footer row.
    let footer_node = crate::add(&card_container, |_state, theme| {
        let mut footer_style = container_style(theme, true);
        footer_style.justify_content = Some(Justify::End.content());
        footer_style.gap = aegle_layout::Size {
            width: LengthPercentage::length(theme.gap),
            height: LengthPercentage::length(0.0),
        };
        Ok((Box::new(aegle_ui::Plain) as Box<dyn Control>, footer_style))
    });

    // 6. Record dialog entry.
    overlay_node.change(|state, _| {
        crate::group::elevate(state, card_node.id)?;
        dialogs(state).entries.push(DialogEntry {
            dialog: overlay_node.id,
            card: card_node.id,
            body: body_col.0.id,
            footer: footer_node.id,
            title_label: title_label.0.id,
            shown: false,
            dismissible: true,
            restore_focus: None,
        });
        Ok(())
    });

    Dialog(overlay_node)
}
