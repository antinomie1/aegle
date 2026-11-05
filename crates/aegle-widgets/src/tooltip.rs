//! Tooltips: a short hint near the pointer after it rests on a control, also
//! given to assistive technology as the control's description. A host whose
//! windows are too small to hold them, such as a panel, shows them itself:
//! see [`host_tooltips`].

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use aegle_controls::{Key, KeyInput};
use aegle_core::NodeId;
use aegle_types::{Point, Rect};
use aegle_ui::{Control, Node, Result, State, Ui};
use aegle_ui::{Insets, LayoutStyle, Length};

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
    /// The host shows tooltips: none are drawn in the window.
    host: bool,
    /// The anchor whose tooltip the host shows.
    hosted: Option<NodeId>,
    /// What the host has not been told yet.
    report: Option<Option<HostTooltip>>,
}

/// A tooltip for the host to show outside the window.
#[derive(Clone, Debug, PartialEq)]
pub struct HostTooltip {
    /// Its text.
    pub text: String,
    /// The control it describes, in window coordinates.
    pub anchor: Rect,
    /// The pointer, in window coordinates.
    pub pointer: Point,
}

/// Has the host show `ui`'s tooltips: they are reported by
/// [`take_tooltip`] instead of drawn inside the window.
pub fn host_tooltips(ui: &Ui) -> Result {
    ui.with_state(|state| {
        state.install(&crate::HOOKS);
        state.ext::<Tooltips>().host = true;
        Ok(())
    })
}

/// What changed since the last call for a window with [`host_tooltips`]:
/// `Some(Some(tip))` to show, `Some(None)` to hide, `None` for no change.
pub fn take_tooltip(ui: &Ui) -> Option<Option<HostTooltip>> {
    ui.with_state(|state| Ok(state.ext::<Tooltips>().report.take()))
        .ok()
        .flatten()
}

pub(crate) fn set_tooltip(node: &Node, text: Option<&str>) {
    // Empty text is no tooltip, so a bound one can come and go; an empty
    // bubble would show otherwise.
    let text = text.filter(|text| !text.is_empty());
    node.set_accessible_description(text);
    node.change(|state, id| {
        state.install(&crate::HOOKS);
        match text {
            Some(text) => {
                state.ext::<Tooltips>().texts.insert(id, text.to_owned());
                // A pointer already resting here starts the delay now.
                let hover = state.hover_target();
                hovered(state, hover)?;
            }
            None => {
                let tooltips = state.ext::<Tooltips>();
                tooltips.texts.remove(&id);
                if tooltips.shown.is_some_and(|s| s.0 == id) || tooltips.hosted == Some(id) {
                    hide(state)?;
                }
            }
        }
        Ok(())
    })
}

/// The nearest control at or above `node` with a tooltip.
fn anchor(state: &mut State, mut node: Option<NodeId>) -> Option<NodeId> {
    while let Some(id) = node {
        if state.ext::<Tooltips>().texts.contains_key(&id) {
            return Some(id);
        }
        node = state.parent(id);
    }
    None
}

/// Removes the shown tooltip and any pending one.
pub(crate) fn hide(state: &mut State) -> Result {
    let tooltips = state.ext::<Tooltips>();
    tooltips.pending = None;
    if tooltips.hosted.take().is_some() {
        tooltips.report = Some(None);
    }
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
    if target.is_some() && (tooltips.shown.map(|s| s.0) == target || tooltips.hosted == target) {
        return Ok(());
    }
    hide(state)?;
    if target.is_some() {
        state.ext::<Tooltips>().pending = target;
        let due = Instant::now() + TOOLTIP_DELAY;
        state.wake_at(due);
    }
    Ok(())
}

pub(crate) fn wake(state: &mut State, _: Instant) -> Result {
    let Some(target) = state.ext_ref::<Tooltips>().and_then(|t| t.pending) else {
        return Ok(());
    };
    // Only the control still under the pointer shows its tooltip.
    if anchor(state, state.hover_target()) != Some(target) || !state.usable(target) {
        state.ext::<Tooltips>().pending = None;
        return Ok(());
    }
    let text = state.ext::<Tooltips>().texts[&target].clone();
    let theme = *state.theme_of(target);
    let pointer = state.pointer_position().unwrap_or_default();
    if state.ext::<Tooltips>().host {
        let anchor = state.element(target).bounds();
        let tooltips = state.ext::<Tooltips>();
        tooltips.pending = None;
        tooltips.hosted = Some(target);
        tooltips.report = Some(Some(HostTooltip {
            text,
            anchor,
            pointer,
        }));
        return Ok(());
    }
    let at = Point::new(pointer.x + OFFSET.x, pointer.y + OFFSET.y);
    let mut style = LayoutStyle::default();
    style
        .set_absolute(Some(Insets::new(0.0, Length::Auto, Length::Auto, 0.0)))
        .set_max_width(320.0);
    let mut control = LabelControl::new(state.fonts(), &text, &theme)?;
    control.kind = &crate::kinds::TOOLTIP;
    let root = state.root();
    let label = state.insert(
        root,
        usize::MAX,
        Box::new(control) as Box<dyn Control>,
        style,
    )?;
    crate::group::elevate(state, label)?;
    let element = state.element_mut(label);
    element.set_padding(Some(theme.padding / 2.0));
    element.set_offset(at);
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
    let size = state.layout_bounds(label).size;
    let x = at.x.min(state.size().width - size.width).max(0.0);
    // Flip above the pointer when there is no room below.
    let y = if at.y + size.height > state.size().height {
        (at.y - OFFSET.y * 2.0 - size.height).max(0.0)
    } else {
        at.y
    };
    let element = state.element_mut(label);
    let moved = element.offset() != Point::new(x, y);
    element.set_offset(Point::new(x, y));
    moved
}

/// Escape dismisses a shown tooltip without reaching the focused control.
pub(crate) fn tooltip_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    if state
        .ext_ref::<Tooltips>()
        .is_none_or(|t| t.shown.is_none() && t.hosted.is_none() && t.pending.is_none())
    {
        return Ok(false);
    }
    let shown = state
        .ext_ref::<Tooltips>()
        .is_some_and(|t| t.shown.is_some() || t.hosted.is_some());
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
    if tooltips.hosted == Some(node) {
        tooltips.hosted = None;
        tooltips.report = Some(None);
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
        if state.is_alive(label) {
            state.remove_subtree(label)?;
        }
    }
    Ok(())
}
