//! Tooltips: a short hint near the pointer after it rests on a control, also
//! given to assistive technology as the control's description.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use aegle_controls::{Key, KeyInput};
use aegle_core::NodeId;
use aegle_layout::{Edges, LengthPercentageAuto, Position, Style};
use aegle_types::Point;
use aegle_ui::{Control, Node, Result, State};

use crate::label::LabelControl;

/// How long the pointer rests before a tooltip shows.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
/// Offset of the tooltip from the pointer, clear of the cursor image.
const OFFSET: Point = Point { x: 12.0, y: 18.0 };

#[derive(Default)]
struct Tooltips {
    texts: HashMap<NodeId, String>,
    /// The anchor waiting for its delay.
    pending: Option<NodeId>,
    /// The anchor shown, its label node and the wanted position.
    shown: Option<(NodeId, NodeId, Point)>,
    /// A shown label whose anchor was removed, to remove after that removal.
    orphan: Option<NodeId>,
}

/// Tooltips on any control.
pub trait NodeTooltip {
    /// Shows `text` after the pointer rests on this control (or a descendant
    /// without its own tooltip) and sets it as the accessible description;
    /// `None` removes it. Pressing, Escape or leaving hides it.
    fn set_tooltip<'a>(&self, text: impl Into<Option<&'a str>>) -> Result;
}

impl NodeTooltip for Node {
    fn set_tooltip<'a>(&self, text: impl Into<Option<&'a str>>) -> Result {
        let text = text.into();
        self.set_accessible_description(text)?;
        self.change(|state, id| {
            state.install(&crate::HOOKS);
            match text {
                Some(text) => {
                    state.ext::<Tooltips>().texts.insert(id, text.to_owned());
                    // A pointer already resting here starts the delay now.
                    let hover = state.hover;
                    hovered(state, hover)?;
                }
                None => {
                    state.ext::<Tooltips>().texts.remove(&id);
                    if state.ext::<Tooltips>().shown.is_some_and(|s| s.0 == id) {
                        hide(state)?;
                    }
                }
            }
            Ok(())
        })
    }
}

/// The nearest control at or above `node` with a tooltip.
fn anchor(state: &mut State, mut node: Option<NodeId>) -> Option<NodeId> {
    while let Some(id) = node {
        if state.ext::<Tooltips>().texts.contains_key(&id) {
            return Some(id);
        }
        node = state.tree.parent(id).ok().flatten();
    }
    None
}

/// Removes the shown tooltip and any pending one.
pub(crate) fn hide(state: &mut State) -> Result {
    let tooltips = state.ext::<Tooltips>();
    tooltips.pending = None;
    if let Some((_, label, _)) = tooltips.shown.take() {
        state.remove_subtree(label)?;
    }
    Ok(())
}

pub(crate) fn hovered(state: &mut State, hit: Option<NodeId>) -> Result {
    if state.ext_ref::<Tooltips>().is_none() {
        return Ok(());
    }
    let target = anchor(state, hit);
    let tooltips = state.ext::<Tooltips>();
    if target.is_some() && tooltips.shown.map(|s| s.0) == target {
        return Ok(());
    }
    hide(state)?;
    if target.is_some() {
        state.ext::<Tooltips>().pending = target;
        let due = Instant::now() + TOOLTIP_DELAY;
        state.wake = Some(state.wake.map_or(due, |wake| wake.min(due)));
    }
    Ok(())
}

pub(crate) fn wake(state: &mut State, _: Instant) -> Result {
    let Some(target) = state.ext_ref::<Tooltips>().and_then(|t| t.pending) else {
        return Ok(());
    };
    // Only the control still under the pointer shows its tooltip.
    if anchor(state, state.hover) != Some(target) || !state.usable(target) {
        state.ext::<Tooltips>().pending = None;
        return Ok(());
    }
    let text = state.ext::<Tooltips>().texts[&target].clone();
    let theme = *state.theme_of(target);
    let pointer = state.pointer.map_or(Point::default(), |(_, p)| p);
    let at = Point::new(pointer.x + OFFSET.x, pointer.y + OFFSET.y);
    let style = Style {
        position: Position::Absolute,
        inset: Edges {
            left: LengthPercentageAuto::length(0.0),
            top: LengthPercentageAuto::length(0.0),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        max_size: aegle_layout::Size {
            width: LengthPercentageAuto::length(320.0),
            height: LengthPercentageAuto::auto(),
        },
        ..Default::default()
    };
    let mut control = LabelControl::new(&state.fonts, &text, &theme)?;
    control.kind = &crate::kinds::TOOLTIP;
    let root = state.root;
    let label = state.insert(
        root,
        usize::MAX,
        Box::new(control) as Box<dyn Control>,
        style,
    )?;
    let element = &mut state.tree.get_mut(label).unwrap().context;
    element.padding = Some(theme.padding / 2.0);
    element.offset = at;
    let tooltips = state.ext::<Tooltips>();
    tooltips.pending = None;
    tooltips.shown = Some((target, label, at));
    Ok(())
}

/// Keeps a shown tooltip inside the window; returns whether it moved.
pub(crate) fn place(state: &mut State) -> bool {
    let Some((_, label, at)) = state.ext_ref::<Tooltips>().and_then(|t| t.shown) else {
        return false;
    };
    let size = state.tree.get(label).unwrap().bounds().size;
    let x = at.x.min(state.size.width - size.width).max(0.0);
    // Flip above the pointer when there is no room below.
    let y = if at.y + size.height > state.size.height {
        (at.y - OFFSET.y * 2.0 - size.height).max(0.0)
    } else {
        at.y
    };
    let element = &mut state.tree.get_mut(label).unwrap().context;
    let moved = element.offset != Point::new(x, y);
    element.offset = Point::new(x, y);
    moved
}

/// Escape dismisses a shown tooltip without reaching the focused control.
pub(crate) fn tooltip_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    if state
        .ext_ref::<Tooltips>()
        .is_none_or(|t| t.shown.is_none() && t.pending.is_none())
    {
        return Ok(false);
    }
    let shown = state
        .ext_ref::<Tooltips>()
        .is_some_and(|t| t.shown.is_some());
    hide(state)?;
    Ok(shown && key.key == Key::Escape)
}

pub(crate) fn removed(state: &mut State, node: NodeId) {
    if state.ext_ref::<Tooltips>().is_none() {
        return;
    }
    let tooltips = state.ext::<Tooltips>();
    tooltips.texts.remove(&node);
    if tooltips.pending == Some(node) {
        tooltips.pending = None;
    }
    match tooltips.shown {
        Some((_, label, _)) if label == node => tooltips.shown = None,
        // The label is not in the removed subtree; drop it afterwards.
        Some((anchor, label, _)) if anchor == node => {
            tooltips.shown = None;
            tooltips.orphan = Some(label);
        }
        _ => {}
    }
}

/// Removes a label whose anchor was removed.
pub(crate) fn prune(state: &mut State) -> Result {
    let orphan = state.ext_ref::<Tooltips>().and_then(|t| t.orphan);
    if let Some(label) = orphan {
        state.ext::<Tooltips>().orphan = None;
        if state.tree.get(label).is_some() {
            state.remove_subtree(label)?;
        }
    }
    Ok(())
}
