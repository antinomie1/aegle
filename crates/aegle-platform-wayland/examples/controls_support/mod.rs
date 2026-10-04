//! Application-specific composition of independent retained modules.

mod input;

use aegle_controls::{Button, TextField};
use aegle_core::{Dirty, Focus, FocusPolicy, NodeId, Route, Tree};
use aegle_layout::{
    AvailableSpace, Dimension, Edges, FlexDirection, LayoutNode, LengthPercentage,
    LengthPercentageAuto, Size, Style,
};
use aegle_platform_wayland::{ImeCause, ImeHints, ImeRequest, PixelSize, WlSeat};
use aegle_scene::{Affine, Color, Rect, RoundedRect, Scene};
use aegle_text::{Alignment, Blob, EditorOptions, EditorPaint, Paragraph, TextStyle, TextSystem};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const INSET: f32 = 14.0;
const ACCENT: Color = Color::rgb(49, 94, 170);

// Large text state is allocated only for nodes that actually need it.
enum Content {
    Container,
    Label(Box<Paragraph>),
    Field(Box<TextField>),
    Button(Button, Box<Paragraph>),
}
struct Element {
    content: Content,
    scene: Scene,
}
impl Element {
    fn new(content: Content) -> Self {
        Self {
            content,
            scene: Scene::default(),
        }
    }
}

pub struct App {
    tree: Tree<LayoutNode<Element>>,
    root: NodeId,
    field: NodeId,
    button: NodeId,
    leaves: [NodeId; 5],
    fonts: TextSystem,
    focus: Focus,
    route: Route,
    capture: Option<(aegle_controls::PointerId, NodeId)>,
    hover: Option<NodeId>,
    pub seat: Option<WlSeat>,
    pub modifiers: aegle_controls::Modifiers,
    pub ime_sync: bool,
    pub ime_reset: bool,
    pub cause: ImeCause,
    scroll: f32,
}

impl App {
    pub fn new() -> Result<Self> {
        let mut fonts = TextSystem::new();
        fonts.register_fonts(Blob::new(Arc::new(
            include_bytes!("../../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        )))?;
        let style = TextStyle {
            families: "Aegle Test CJK",
            size: 15.0,
            color: Color::rgb(36, 45, 62),
            ..Default::default()
        };
        let mut tree = Tree::with_capacity(6);
        let root = tree.insert(None, LayoutNode::new(Element::new(Content::Container)))?;
        let mut label = |text, size, height| -> Result<NodeId> {
            let paragraph = fonts.paragraph(
                text,
                &TextStyle {
                    size,
                    ..style.clone()
                },
            )?;
            Ok(tree.insert(
                Some(root),
                LayoutNode::with_style(
                    fixed_height(height),
                    Element::new(Content::Label(Box::new(paragraph))),
                ),
            )?)
        };
        let title = label("Aegle / retained controls", 20.0, 30.0)?;
        let subtitle = label(
            "One tree. Shared layout, input and CJK editing.",
            13.0,
            21.0,
        )?;
        let editor = fonts.editor(
            "Hello, 世界\n你好 / 日本語 / 한글",
            &TextStyle {
                size: 23.0,
                ..style.clone()
            },
            EditorOptions {
                multiline: true,
                ..Default::default()
            },
        )?;
        let field = tree.insert(
            Some(root),
            LayoutNode::with_style(
                Style {
                    flex_grow: 1.0,
                    min_size: Size {
                        width: LengthPercentageAuto::length(0.0),
                        height: LengthPercentageAuto::length(72.0),
                    },
                    ..Default::default()
                },
                Element::new(Content::Field(Box::new(TextField::new(editor)))),
            ),
        )?;
        let button = tree.insert(
            Some(root),
            LayoutNode::with_style(
                Style {
                    size: Size {
                        width: Dimension::length(156.0),
                        height: Dimension::length(42.0),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                Element::new(Content::Button(
                    Button::new(),
                    Box::new(fonts.paragraph("Clear text", &style)?),
                )),
            ),
        )?;
        let help = tree.insert(
            Some(root),
            LayoutNode::with_style(
                fixed_height(18.0),
                Element::new(Content::Label(Box::new(fonts.paragraph(
                    "Tab: focus   Shift+Tab: back   Space: activate",
                    &TextStyle {
                        size: 12.0,
                        ..style
                    },
                )?))),
            ),
        )?;
        Ok(Self {
            tree,
            root,
            field,
            button,
            leaves: [title, subtitle, field, button, help],
            fonts,
            focus: Focus::new(),
            route: Route::new(),
            capture: None,
            hover: None,
            seat: None,
            modifiers: Default::default(),
            ime_sync: true,
            ime_reset: false,
            cause: ImeCause::Other,
            scroll: 0.0,
        })
    }

    pub fn resize(&mut self, size: PixelSize) -> Result<()> {
        let next = Size {
            width: Dimension::length(size.width as f32),
            height: Dimension::length(size.height as f32),
        };
        if self.tree.get(self.root).unwrap().style().size == next {
            return Ok(());
        }
        aegle_layout::set_style(
            &mut self.tree,
            self.root,
            Style {
                size: Size {
                    width: Dimension::length(size.width as f32),
                    height: Dimension::length(size.height as f32),
                },
                flex_direction: FlexDirection::Column,
                padding: Edges {
                    left: LengthPercentage::length(28.0),
                    right: LengthPercentage::length(28.0),
                    top: LengthPercentage::length(24.0),
                    bottom: LengthPercentage::length(24.0),
                },
                gap: Size {
                    width: LengthPercentage::length(12.0),
                    height: LengthPercentage::length(12.0),
                },
                ..Default::default()
            },
        )?;
        self.ime_sync = true;
        Ok(())
    }

    /// Reflow and records reuse the exact tree later used for hit testing.
    pub fn refresh(&mut self, size: PixelSize) -> Result<bool> {
        let Content::Field(field) = &mut self.tree.get_mut(self.field).unwrap().context.content
        else {
            unreachable!()
        };
        let previous = field.editor_mut().take_changes();
        if previous.layout {
            self.tree.mark_dirty(self.field, Dirty::LAYOUT)?;
        }
        if self.tree.dirty(self.root)?.intersects(Dirty::LAYOUT) {
            aegle_layout::compute(
                &mut self.tree,
                self.root,
                Size {
                    width: AvailableSpace::Definite(size.width as f32),
                    height: AvailableSpace::Definite(size.height as f32),
                },
                |_, element, known, _| {
                    let size = match &element.content {
                        Content::Label(text) | Content::Button(_, text) => text.size(),
                        Content::Field(field) => field.editor().size(),
                        Content::Container => aegle_types::Size::default(),
                    };
                    Size {
                        width: known.width.unwrap_or(size.width),
                        height: known.height.unwrap_or(size.height),
                    }
                },
            )?;
            for id in self.leaves {
                self.tree.mark_dirty(id, Dirty::PAINT)?;
            }
            let width = (self.bounds(self.field).size.width - INSET * 2.0).max(1.0);
            let Content::Field(field) = &mut self.tree.get_mut(self.field).unwrap().context.content
            else {
                unreachable!()
            };
            self.fonts
                .edit(field.editor_mut())
                .reflow(Some(width), Alignment::Start)?;
        }
        let height = (self.bounds(self.field).size.height - INSET * 2.0).max(1.0);
        let Content::Field(field) = &mut self.tree.get_mut(self.field).unwrap().context.content
        else {
            unreachable!()
        };
        let changes = field.editor_mut().take_changes();
        if changes.layout || changes.selection || previous.layout || previous.selection {
            let caret = field.editor().ime_rect();
            self.scroll = self
                .scroll
                .min(caret.origin.y)
                .max(caret.origin.y + caret.size.height - height);
        }
        self.scroll = self
            .scroll
            .clamp(0.0, (field.editor().size().height - height).max(0.0));
        if changes.value
            || changes.layout
            || changes.selection
            || changes.policy
            || previous.value
            || previous.layout
            || previous.selection
            || previous.policy
        {
            self.tree
                .mark_dirty(self.field, Dirty::PAINT | Dirty::SEMANTICS)?;
            self.ime_sync = true;
        }
        let mut changed = false;
        for id in self.leaves {
            if self.tree.dirty(id)?.intersects(Dirty::PAINT) {
                self.record(id)?;
                self.tree.clear_dirty(id, Dirty::PAINT)?;
                changed = true;
            }
        }
        Ok(changed)
    }

    fn record(&mut self, id: NodeId) -> Result<()> {
        let node = self.tree.get_mut(id).unwrap();
        let size = node.bounds().size;
        let mut builder = std::mem::take(&mut node.context.scene).into_builder();
        builder.clear();
        match &node.context.content {
            Content::Label(label) => {
                label.paint(&mut builder)?;
            }
            Content::Field(field) => {
                let shape = RoundedRect::new(
                    Rect::new(
                        0.5,
                        0.5,
                        (size.width - 1.0).max(0.0),
                        (size.height - 1.0).max(0.0),
                    ),
                    8.0,
                )?;
                builder.fill(shape, Color::WHITE)?;
                builder.stroke(
                    shape,
                    if field.is_focused() {
                        ACCENT
                    } else {
                        Color::rgb(201, 208, 217)
                    },
                    1.0,
                )?;
                builder.push_clip(shape)?;
                builder.push_transform(Affine::translation(INSET, INSET - self.scroll)?)?;
                field.editor().paint(
                    &mut builder,
                    EditorPaint {
                        caret: field.is_focused().then_some(ACCENT),
                        preedit: Some(ACCENT),
                        ..Default::default()
                    },
                )?;
                builder.pop()?.pop()?;
            }
            Content::Button(button, label) => {
                let shape = RoundedRect::new(
                    Rect::new(1.0, 1.0, size.width - 2.0, size.height - 2.0),
                    7.0,
                )?;
                let color = if button.is_pressed() {
                    Color::rgb(213, 224, 240)
                } else if button.is_hovered() {
                    Color::rgb(232, 237, 246)
                } else {
                    Color::WHITE
                };
                builder.fill(shape, color)?;
                builder.stroke(
                    shape,
                    if button.is_focused() {
                        ACCENT
                    } else {
                        Color::rgb(199, 207, 220)
                    },
                    1.0,
                )?;
                let label_size = label.size();
                builder.push_transform(Affine::translation(
                    (size.width - label_size.width) / 2.0,
                    (size.height - label_size.height) / 2.0,
                )?)?;
                label.paint(&mut builder)?;
                builder.pop()?;
            }
            Content::Container => {}
        }
        node.context.scene = builder.finish()?;
        Ok(())
    }

    pub fn records(&self) -> impl Iterator<Item = (&Scene, aegle_types::Point)> {
        self.leaves.into_iter().map(|id| {
            let node = self.tree.get(id).unwrap();
            (&node.context.scene, node.bounds().origin)
        })
    }

    pub fn ime_request(&self) -> Option<ImeRequest> {
        let Content::Field(field) = &self.tree.get(self.field).unwrap().context.content else {
            unreachable!()
        };
        if !field.accepts_ime() {
            return None;
        }
        let surrounding = field.editor().surrounding(4000);
        let selection = surrounding.map(|text| text.selection).unwrap_or_default();
        let mut cursor_rect = field.editor().ime_rect();
        let origin = self.bounds(self.field).origin;
        cursor_rect.origin.x += origin.x + INSET;
        cursor_rect.origin.y += origin.y + INSET - self.scroll;
        Some(ImeRequest {
            surrounding: surrounding.map(|text| text.to_string()),
            cursor: selection.focus,
            anchor: selection.anchor,
            cursor_rect,
            hints: ImeHints::Multiline,
            cause: self.cause,
            ..Default::default()
        })
    }

    fn bounds(&self, id: NodeId) -> Rect {
        self.tree.get(id).unwrap().bounds()
    }
}

fn fixed_height(height: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::auto(),
            height: Dimension::length(height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn focus_policy(_: NodeId, node: &LayoutNode<Element>) -> FocusPolicy {
    match &node.context.content {
        Content::Field(field) if field.is_enabled() => FocusPolicy::Focusable,
        Content::Button(button, _) if button.is_enabled() => FocusPolicy::Focusable,
        Content::Container | Content::Label(_) => FocusPolicy::Skip,
        _ => FocusPolicy::Prune,
    }
}
