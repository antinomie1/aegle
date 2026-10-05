use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use aegle_controls::{Button, PointerId, TextField};
use aegle_core::{Focus, NodeId, Route, Tree};
use aegle_layout::LayoutNode;
use aegle_scene::Scene;
use aegle_text::{Paragraph, TextStyle, TextSystem};
use aegle_theme::Theme;
use aegle_types::{Point, Rect, Size};

use crate::{Result, callbacks::Handler, style::Decoration};

pub(crate) enum Content {
    Container,
    /// A viewport and its overlay record, drawn after the viewport's subtree.
    Scroll(Box<Scene>),
    Label(Box<Paragraph>),
    Button(Button, Box<Paragraph>),
    Field(Box<TextField>),
    Toggle(Box<ToggleContent>),
    Slider(Box<aegle_controls::Slider>),
    Progress(aegle_controls::Range),
}

pub(crate) struct ToggleContent {
    pub control: aegle_controls::Toggle,
    pub text: Paragraph,
    pub switch: bool,
}

impl Content {
    pub fn interactive(&self) -> bool {
        matches!(
            self,
            Self::Button(..) | Self::Field(_) | Self::Toggle(_) | Self::Slider(_)
        )
    }
    pub fn paragraph(&self) -> Option<&Paragraph> {
        match self {
            Self::Label(text) | Self::Button(_, text) => Some(text),
            Self::Toggle(toggle) => Some(&toggle.text),
            _ => None,
        }
    }
    pub fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        match self {
            Self::Label(text) | Self::Button(_, text) => Some(text),
            Self::Toggle(toggle) => Some(&mut toggle.text),
            _ => None,
        }
    }
}

pub(crate) struct Element {
    pub content: Content,
    pub scene: Scene,
    pub bounds: Rect,
    pub clip: Option<Rect>,
    pub scroll: Point,
    pub visible: bool,
    pub effective_visible: bool,
    pub enabled: bool,
    pub label: String,
    pub ensure_caret: bool,
    pub local_layout: u8,
    pub padding: Option<f32>,
    #[cfg(feature = "accessibility")]
    pub access_id: aegle_access::accesskit::NodeId,
}

impl Element {
    pub fn inset(&self, default: f32) -> f32 {
        self.padding
            .unwrap_or(if matches!(self.content, Content::Label(_)) {
                0.0
            } else {
                default
            })
    }
    pub fn new(content: Content) -> Self {
        Self {
            content,
            scene: Scene::default(),
            bounds: Rect::default(),
            clip: None,
            scroll: Point::default(),
            visible: true,
            effective_visible: true,
            enabled: true,
            label: String::new(),
            ensure_caret: false,
            local_layout: 0,
            padding: None,
            #[cfg(feature = "accessibility")]
            access_id: aegle_access::accesskit::NodeId(0),
        }
    }
}

pub(crate) struct State {
    pub tree: Tree<LayoutNode<Element>>,
    pub root: NodeId,
    pub order: Vec<NodeId>,
    /// Scroll views in post-order with the end of their subtree in `order`.
    pub overlays: Vec<(usize, NodeId)>,
    pub topology_dirty: bool,
    pub geometry_dirty: bool,
    pub reveal_target: Option<NodeId>,
    pub fonts: Rc<RefCell<TextSystem>>,
    pub theme: Theme,
    pub size: Size,
    pub focus: Focus,
    pub last_focus: Option<NodeId>,
    pub route: Route,
    pub capture: Option<(PointerId, NodeId)>,
    pub drag: Option<crate::scrollbar::Drag>,
    pub hover: Option<NodeId>,
    pub pointer: Option<(PointerId, Point)>,
    pub ime_dirty: bool,
    pub ime_reset: bool,
    pub input_method: bool,
    pub clipboard: Option<crate::ClipboardRequest>,
    pub repaint: bool,
    pub callbacks: HashMap<NodeId, Handler>,
    pub decorations: HashMap<NodeId, Decoration>,
    #[cfg(feature = "motion")]
    pub motion: crate::motion::Motion,
    pub pending: VecDeque<(NodeId, u64)>,
    pub dispatching: bool,
    pub callback_version: u64,
    #[cfg(feature = "accessibility")]
    pub next_access_id: u64,
}

impl State {
    pub fn style(&self) -> TextStyle<'static> {
        TextStyle {
            size: self.theme.font_size,
            color: self.theme.foreground,
            ..Default::default()
        }
    }

    pub fn rebuild_order(&mut self) {
        if !self.topology_dirty {
            return;
        }
        self.order.clear();
        self.overlays.clear();
        self.order.push(self.root);
        let mut stack = vec![(self.root, 0)];
        while let Some((parent, next)) = stack.last_mut() {
            if let Some(child) = self.tree.child(*parent, *next).unwrap() {
                *next += 1;
                self.order.push(child);
                stack.push((child, 0));
            } else {
                let (id, _) = stack.pop().unwrap();
                if matches!(
                    self.tree.get(id).unwrap().context.content,
                    Content::Scroll(_)
                ) {
                    self.overlays.push((self.order.len(), id));
                }
            }
        }
        self.topology_dirty = false;
    }

    pub fn contains(&self, root: NodeId, mut node: NodeId) -> bool {
        loop {
            if root == node {
                return true;
            }
            match self.tree.parent(node).ok().flatten() {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    pub fn usable(&self, mut id: NodeId) -> bool {
        loop {
            let Some(node) = self.tree.get(id) else {
                return false;
            };
            if !node.context.visible || !node.context.enabled {
                return false;
            }
            match self.tree.parent(id).unwrap() {
                Some(parent) => id = parent,
                None => return true,
            }
        }
    }

    pub fn invalidate_structure(&mut self) {
        self.topology_dirty = true;
        self.repaint = true;
        self.ime_dirty = true;
    }

    pub fn insert(
        &mut self,
        parent: NodeId,
        content: Content,
        mut style: aegle_layout::Style,
    ) -> Result<NodeId> {
        // These controls clip their own contents and manage any text scrolling
        // internally. Their intrinsic overflow must not enlarge an ancestor view.
        if matches!(
            content,
            Content::Button(..)
                | Content::Field(_)
                | Content::Toggle(_)
                | Content::Slider(_)
                | Content::Progress(_)
        ) {
            style.overflow.x = aegle_layout::Overflow::Hidden;
            style.overflow.y = aegle_layout::Overflow::Hidden;
        }
        let mut element = Element::new(content);
        #[cfg(feature = "accessibility")]
        {
            element.access_id = aegle_access::accesskit::NodeId(self.next_access_id);
            self.next_access_id = self
                .next_access_id
                .checked_add(1)
                .ok_or(crate::UiError::IdentityExhausted)?;
        }
        #[cfg(not(feature = "accessibility"))]
        let _ = &mut element;
        let id = self
            .tree
            .insert(Some(parent), LayoutNode::with_style(style, element))?;
        #[cfg(feature = "motion")]
        if self.tree.get(id).unwrap().context.content.interactive() {
            if let Some(timing) = self.motion.default {
                self.motion.tracks.insert(
                    id,
                    crate::motion::Track {
                        timing,
                        presented: None,
                    },
                );
            }
        }
        self.invalidate_structure();
        Ok(id)
    }
}

pub(crate) fn focus_policy(_: NodeId, node: &LayoutNode<Element>) -> aegle_core::FocusPolicy {
    use aegle_core::FocusPolicy;
    if !node.context.visible || !node.context.enabled {
        return FocusPolicy::Prune;
    }
    if node.context.content.interactive() {
        FocusPolicy::Focusable
    } else {
        FocusPolicy::Skip
    }
}
