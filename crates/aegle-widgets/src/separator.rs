//! Thin dividers and resizable two-pane splitters.

use std::{cell::Cell, rc::Rc};

use aegle_controls::Key;
use aegle_layout::{AlignItems, Dimension, FlexDirection, Style};
use aegle_scene::{Rect, RoundedRect};
use aegle_theme::ControlKind;
use aegle_types::Cursor;
use aegle_ui::{
    Container, Control, Length, Result, UiError,
    control::{Frame, PaintCx},
    handle,
};

use crate::{Canvas, CanvasEvent, Orientation, Widgets};

handle! {
    /// A one-pixel divider across its row (vertical) or column (horizontal); it is not focusable.
    pub Separator(SeparatorControl)
}

/// The control inside a [`Separator`] node.
pub struct SeparatorControl {
    /// Reported to assistive technology; layout already encodes it.
    #[cfg(feature = "accessibility")]
    vertical: bool,
}

impl Control for SeparatorControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        let rect = Rect::new(0.0, 0.0, cx.size.width, cx.size.height);
        if !rect.is_empty() {
            cx.builder
                .fill(RoundedRect::new(rect, 0.0)?, cx.theme.border)?;
        }
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        // AccessKit's Splitter role is ARIA's separator.
        cx.node.set_role(aegle_ui::accesskit::Role::Splitter);
        cx.node.set_orientation(if self.vertical {
            aegle_ui::accesskit::Orientation::Vertical
        } else {
            aegle_ui::accesskit::Orientation::Horizontal
        });
    }
}

fn line(vertical: bool) -> Style {
    let one = Dimension::length(1.0);
    Style {
        size: if vertical {
            aegle_layout::Size {
                width: one,
                height: Dimension::auto(),
            }
        } else {
            aegle_layout::Size {
                width: Dimension::auto(),
                height: one,
            }
        },
        align_self: Some(AlignItems::STRETCH),
        flex_shrink: 0.0,
        ..Default::default()
    }
}

pub(crate) fn separator(container: &Container) -> Separator {
    Separator(crate::add(container, |state, _| {
        // Across a row the divider is vertical, across a column horizontal.
        let parent = container.id;
        let vertical = matches!(
            state.tree.get(parent).unwrap().style().flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );
        Ok((
            Box::new(SeparatorControl {
                #[cfg(feature = "accessibility")]
                vertical,
            }) as Box<dyn Control>,
            line(vertical),
        ))
    }))
}

/// Two panes divided by a draggable, keyboard-adjustable handle.
#[derive(Clone)]
pub struct Splitter {
    root: Container,
    first: Container,
    second: Container,
    handle: Canvas,
    ratio: Rc<Cell<f32>>,
}

impl std::ops::Deref for Splitter {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.root
    }
}

/// Handle thickness in logical pixels.
const GRIP: f32 = 6.0;
/// Arrow-key adjustment as a fraction of the splitter.
const KEY_STEP: f32 = 0.02;

impl Splitter {
    /// The first (left or top) pane.
    pub fn first(&self) -> &Container {
        &self.first
    }
    /// The second (right or bottom) pane.
    pub fn second(&self) -> &Container {
        &self.second
    }
    /// The share of the space given to the first pane, `0.0..=1.0`.
    pub fn ratio(&self) -> f32 {
        self.ratio.get()
    }
    /// Gives `ratio` (finite, `0.0..=1.0`) of the space to the first pane.
    pub fn set_ratio(&self, ratio: f32) {
        if !(0.0..=1.0).contains(&ratio) {
            panic!("{}", UiError::InvalidValue);
        }
        self.ratio.set(ratio);
        self.first.set_basis(Length::Percent(ratio * 100.0));
        self.handle.update(|grip| {
            if let Some((_, share)) = &mut grip.splitter {
                *share = ratio;
            }
        })
    }
}

pub(crate) fn splitter(container: &Container, orientation: Orientation) -> Splitter {
    let vertical = orientation == Orientation::Vertical;
    let root = if vertical {
        container.column()
    } else {
        container.row()
    };
    root.set_gap(0.0, 0.0);
    root.set_grow(1.0);
    root.set_min_width(0.0);
    root.set_min_height(0.0);
    let first = root.column();
    let grip = root.canvas(move |builder, size| {
        let line = if vertical {
            Rect::new(0.0, size.height * 0.5 - 0.5, size.width, 1.0)
        } else {
            Rect::new(size.width * 0.5 - 0.5, 0.0, 1.0, size.height)
        };
        builder.fill(
            RoundedRect::new(line, 0.0)?,
            aegle_scene::Color::rgba(128, 128, 128, 160),
        )?;
        Ok(())
    });
    let second = root.column();
    for pane in [&first, &second] {
        pane.set_clip(true);
        pane.set_min_width(0.0);
        pane.set_min_height(0.0);
        pane.set_shrink(1.0);
    }
    second.set_basis(0.0);
    second.set_grow(1.0);
    if vertical {
        grip.set_width(Length::Auto);
        grip.set_height(GRIP);
    } else {
        grip.set_width(GRIP);
        grip.set_height(Length::Auto);
    }
    grip.set_cursor(Some(if vertical {
        Cursor::ResizeVertical
    } else {
        Cursor::ResizeHorizontal
    }));
    grip.set_accessible_label("Resize panes");
    grip.update(|grip| grip.splitter = Some((vertical, 0.5)));
    let ratio = Rc::new(Cell::new(0.5));
    let splitter = Splitter {
        root,
        first,
        second,
        handle: grip.clone(),
        ratio,
    };
    splitter.set_ratio(0.5);
    let this = splitter.clone();
    grip.set_input(move |grip, event| {
        let along = |p: aegle_types::Point| if vertical { p.y } else { p.x };
        // Right to left the first pane is on the right of a horizontal split.
        let mirror = !vertical && grip.layout_direction() == aegle_ui::LayoutDirection::Rtl;
        let target = match event {
            CanvasEvent::Move {
                position,
                pressed: true,
                ..
            } => {
                let (area, at) = (this.root.bounds(), grip.bounds());
                let extent = if vertical {
                    area.size.height
                } else {
                    area.size.width
                } - GRIP;
                let offset = along(at.origin) - along(area.origin) + along(position) - GRIP * 0.5;
                (extent > 0.0).then(|| {
                    let fraction = offset / extent;
                    if mirror { 1.0 - fraction } else { fraction }
                })
            }
            CanvasEvent::Key {
                key, pressed: true, ..
            } => match key {
                Key::Left if mirror => Some(this.ratio() + KEY_STEP),
                Key::Right if mirror => Some(this.ratio() - KEY_STEP),
                Key::Left | Key::Up => Some(this.ratio() - KEY_STEP),
                Key::Right | Key::Down => Some(this.ratio() + KEY_STEP),
                Key::Home => Some(0.0),
                Key::End => Some(1.0),
                _ => None,
            },
            _ => None,
        };
        if let Some(ratio) = target {
            this.set_ratio(ratio.clamp(0.0, 1.0));
        }
    });
    splitter
}
