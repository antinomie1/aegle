//! A slide-out navigation drawer docked to the edge of the window.

use aegle_controls::{Key, KeyInput};
use aegle_core::NodeId;
use aegle_layout::{
    Dimension, Display, Edges, Length, LengthPercentage, LengthPercentageAuto, Position,
};
use aegle_scene::RoundedRect;
use aegle_theme::ControlKind;
use aegle_types::{Color, Point, Rect};
use aegle_ui::{
    Container, Control, HandlerResult, Node, Result, State, container_style, control::PaintCx,
    handle,
};

use crate::{Button, Separator, Widgets};

/// Which edge of the window the drawer docks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawerSide {
    /// Docked to the left edge.
    #[default]
    Left,
    /// Docked to the right edge.
    Right,
}

/// An entry describing an edge-docked drawer in the window.
pub(crate) struct DrawerEntry {
    pub drawer: NodeId,
    pub panel: NodeId,
    pub content: NodeId,
    pub side: DrawerSide,
    pub shown: bool,
    pub restore_focus: Option<NodeId>,
}

/// Storage for navigation drawers in one UI window.
#[derive(Default)]
pub(crate) struct Drawers {
    pub entries: Vec<DrawerEntry>,
}

pub(crate) fn drawers(state: &mut State) -> &mut Drawers {
    state.ext()
}

/// A control that renders the dark semi-transparent scrim behind open drawers.
pub struct DrawerScrimControl;

impl Control for DrawerScrimControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        cx.builder.fill(
            RoundedRect::new(Rect::new(0.0, 0.0, cx.size.width, cx.size.height), 0.0),
            Color::rgba(0, 0, 0, 90),
        );
    }
}

handle! {
    /// A slide-out navigation drawer docked to the window edge.
    pub Drawer
}

impl Drawer {
    /// Opens the drawer and reveals its contents.
    pub fn open(&self) {
        self.0.change(|state, id| {
            let focus = state.focus.current(&state.tree);
            {
                let entries = &mut drawers(state).entries;
                let Some(entry) = entries.iter_mut().find(|e| e.drawer == id) else {
                    return Ok(());
                };
                entry.shown = true;
                entry.restore_focus = focus;
            }
            state.set_visible(id, true)?;
            Ok(())
        });
    }

    /// Alias for `open`.
    pub fn show(&self) {
        self.open();
    }

    /// Closes the drawer and restores previous focus.
    pub fn close(&self) {
        self.0.change(|state, id| {
            let restore = {
                let entries = &mut drawers(state).entries;
                let Some(entry) = entries.iter_mut().find(|e| e.drawer == id) else {
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

    /// Alias for `close`.
    pub fn hide(&self) {
        self.close();
    }

    /// Toggles the drawer between open and closed.
    pub fn toggle(&self) {
        if self.is_open() {
            self.close();
        } else {
            self.open();
        }
    }

    /// Returns whether this drawer is currently open.
    pub fn is_open(&self) -> bool {
        self.0.change(|state, id| {
            Ok(drawers(state)
                .entries
                .iter()
                .find(|e| e.drawer == id)
                .map(|e| e.shown)
                .unwrap_or(false))
        })
    }

    /// Returns the side this drawer docks to.
    pub fn side(&self) -> DrawerSide {
        self.0.change(|state, id| {
            Ok(drawers(state)
                .entries
                .iter()
                .find(|e| e.drawer == id)
                .map(|e| e.side)
                .unwrap_or_default())
        })
    }

    /// Sets the width of the drawer panel in logical pixels.
    pub fn set_width(&self, width: f32) {
        let panel_id = self.0.change(|state, id| {
            Ok(drawers(state)
                .entries
                .iter()
                .find(|e| e.drawer == id)
                .map(|e| e.panel))
        });
        if let Some(panel_id) = panel_id {
            let panel = Node {
                state: self.0.state.clone(),
                id: panel_id,
            };
            panel.set_width(Length::Px(width));
        }
    }

    /// Returns the content container inside the drawer panel.
    pub fn content(&self) -> Container {
        let content_id = self.0.change(|state, id| {
            Ok(drawers(state)
                .entries
                .iter()
                .find(|e| e.drawer == id)
                .map(|e| e.content)
                .unwrap())
        });
        Container(Node {
            state: self.0.state.clone(),
            id: content_id,
        })
    }

    /// Adds a navigation button to the drawer.
    pub fn item(&self, text: &str) -> Button {
        let btn = self.content().button(text);
        btn.set_width(Length::Percent(100.0));
        btn
    }

    /// Adds a divider line to the drawer.
    pub fn separator(&self) -> Separator {
        self.content().separator()
    }

    /// Adds a handler invoked when the drawer closes.
    pub fn on_close<R: HandlerResult>(
        &self,
        mut callback: impl FnMut() -> R + 'static,
    ) -> aegle_ui::Subscription {
        self.on_action(move |_| callback().into_result())
    }
}

/// Dismisses open drawer when clicking on its backdrop scrim.
pub(crate) fn dismiss_drawer(state: &mut State, position: Point) -> Result<bool> {
    let (panel, drawer_id) = {
        let entries = &drawers(state).entries;
        let Some(entry) = entries.iter().rfind(|e| e.shown) else {
            return Ok(false);
        };
        (entry.panel, entry.drawer)
    };

    let inside = state
        .tree
        .get(panel)
        .is_some_and(|n| n.context.bounds.contains(position));

    if !inside {
        close_drawer_by_id(state, drawer_id)?;
        return Ok(true);
    }

    Ok(false)
}

/// Closes drawer by NodeId.
fn close_drawer_by_id(state: &mut State, id: NodeId) -> Result {
    let restore = {
        let entries = &mut drawers(state).entries;
        let Some(entry) = entries.iter_mut().find(|e| e.drawer == id) else {
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

/// Handles Escape to close open drawers.
pub(crate) fn drawer_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    if key.key != Key::Escape {
        return Ok(false);
    }
    let entries = &drawers(state).entries;
    let Some(entry) = entries.iter().rfind(|e| e.shown) else {
        return Ok(false);
    };
    let drawer_id = entry.drawer;
    close_drawer_by_id(state, drawer_id)?;
    Ok(true)
}

/// Cleans up drawer on node removal.
pub(crate) fn removed(state: &mut State, node: NodeId) {
    drawers(state)
        .entries
        .retain(|e| e.drawer != node && e.panel != node);
}

/// Constructs a [`Drawer`] anchored to `container` on `side`.
pub(crate) fn create_drawer(container: &Container, side: DrawerSide) -> Drawer {
    // 1. Overlay node covering full window in a flex row.
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
        overlay_style.flex_direction = aegle_layout::FlexDirection::Row;
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
        Ok((Box::new(aegle_ui::Plain) as Box<dyn Control>, overlay_style))
    });
    overlay_node.set_visible(false);

    let overlay_container = Container(overlay_node.clone());

    // 2. Docked panel and backdrop scrim.
    let panel_group = crate::group::Group {
        role: crate::group::Role::Drawer,
    };

    let make_panel = |c: &Container| {
        crate::add(c, |_state, theme| {
            let mut panel_style = container_style(theme, false);
            panel_style.size = aegle_layout::Size {
                width: Dimension::length(280.0),
                height: Dimension::percent(1.0),
            };
            let p = LengthPercentage::length(theme.padding);
            panel_style.padding = Edges {
                left: p,
                right: p,
                top: p,
                bottom: p,
            };
            panel_style.gap = aegle_layout::Size {
                width: LengthPercentage::length(0.0),
                height: LengthPercentage::length(theme.gap),
            };
            Ok((Box::new(panel_group) as Box<dyn Control>, panel_style))
        })
    };

    let make_scrim = |c: &Container| {
        crate::add(c, |_state, theme| {
            let mut scrim_style = container_style(theme, false);
            scrim_style.flex_grow = 1.0;
            scrim_style.size.height = Dimension::percent(1.0);
            Ok((
                Box::new(DrawerScrimControl) as Box<dyn Control>,
                scrim_style,
            ))
        })
    };

    let panel_node = match side {
        DrawerSide::Left => {
            let p = make_panel(&overlay_container);
            let _s = make_scrim(&overlay_container);
            p
        }
        DrawerSide::Right => {
            let _s = make_scrim(&overlay_container);
            make_panel(&overlay_container)
        }
    };

    overlay_node.change(|state, _| {
        crate::group::elevate(state, panel_node.id)?;
        drawers(state).entries.push(DrawerEntry {
            drawer: overlay_node.id,
            panel: panel_node.id,
            content: panel_node.id,
            side,
            shown: false,
            restore_focus: None,
        });
        Ok(())
    });

    Drawer(overlay_node)
}
