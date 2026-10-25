//! In-window modal dialogs with backdrop scrim and focus management.

use aegle_controls::{Key, KeyInput};
use aegle_core::NodeId;
use aegle_scene::RoundedRect;
use aegle_theme::ControlKind;
use aegle_types::{Color, Point, Rect};
use aegle_ui::{Align, Display, Insets, Justify, Length, Overflow};
use aegle_ui::{
    Container, Control, HandlerResult, OrFail, Result, State, container_style, control::PaintCx,
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
            let focus = state.focused();
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
            let label = Label(self.0.handle(label_id));
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
        Container(self.0.handle(body_id))
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
        Container(self.0.handle(footer_id))
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

    let inside = state.is_alive(card) && state.element(card).bounds().contains(position);

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

/// The card's preferred minimum width.
const MIN_WIDTH: f32 = 320.0;
/// Space between the card and the edges of the area its scrim covers.
const MARGIN: f32 = 16.0;

/// After layout: lowers each open card's minimum width to the space its scrim
/// leaves, so a narrow window keeps the whole card. Returns whether any changed.
pub(crate) fn fit(state: &mut State) -> bool {
    let Some(dialogs) = state.ext_ref::<Dialogs>() else {
        return false;
    };
    let open: Vec<_> = (dialogs.entries.iter())
        .filter(|e| e.shown)
        .map(|e| (e.dialog, e.card))
        .collect();
    let mut changed = false;
    for (dialog, card) in open {
        let room = state.layout_bounds(dialog).size.width - 2.0 * MARGIN;
        let old = state.layout_style(card);
        let mut style = old.clone();
        style.set_min_width(MIN_WIDTH.min(room).max(0.0));
        if style != *old {
            state.set_layout_style(card, style).or_fail();
            changed = true;
        }
    }
    changed
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
        overlay_style
            .set_absolute(Some(Insets::all(0.0)))
            .set_display(Display::Flex)
            .set_width(Length::Percent(100.0))
            .set_height(Length::Percent(100.0))
            .set_gap(0.0, 0.0)
            // The card keeps this margin from the covered area's edges.
            .set_padding(MARGIN)
            .set_align_items(Some(Align::Center))
            .set_justify_content(Some(Justify::Center));
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
        card_style
            .set_min_width(MIN_WIDTH)
            .set_max_width(560.0)
            // Content taller or wider than the covered area scrolls in the card.
            .set_overflow(Overflow::Scroll, Overflow::Scroll)
            .set_padding(theme.padding * 1.5)
            .set_gap(theme.gap * 1.25, 0.0);
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
        footer_style
            .set_justify_content(Some(Justify::End))
            .set_gap(0.0, theme.gap);
        Ok((Box::new(aegle_ui::Plain) as Box<dyn Control>, footer_style))
    });

    // 6. Record dialog entry.
    overlay_node.change(|state, _| {
        crate::group::elevate(state, card_node.id())?;
        dialogs(state).entries.push(DialogEntry {
            dialog: overlay_node.id(),
            card: card_node.id(),
            body: body_col.0.id(),
            footer: footer_node.id(),
            title_label: title_label.0.id(),
            shown: false,
            dismissible: true,
            restore_focus: None,
        });
        Ok(())
    });

    Dialog(overlay_node)
}
