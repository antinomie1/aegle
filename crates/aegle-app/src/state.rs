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
    Image(aegle_scene::Image),
    Canvas(Box<crate::visual_handles::Painter>),
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
    /// Presented translation after layout, inherited by the subtree.
    pub offset: Point,
    /// Nearest local theme of this node or an ancestor; `None` uses the UI theme.
    pub theme: Option<Rc<Theme>>,
    /// Whether `theme` was set on this node rather than inherited.
    pub local_theme: bool,
    #[cfg(feature = "accessibility")]
    pub access_id: aegle_access::accesskit::NodeId,
}

impl Element {
    pub fn theme_or<'a>(&'a self, ui: &'a Theme) -> &'a Theme {
        self.theme.as_deref().unwrap_or(ui)
    }
    /// Content padding: the local value, else zero for labels or the theme's.
    pub fn inset(&self, ui: &Theme) -> f32 {
        self.padding
            .unwrap_or(if matches!(self.content, Content::Label(_)) {
                0.0
            } else {
                self.theme_or(ui).padding
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
            offset: Point::default(),
            theme: None,
            local_theme: false,
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
    /// Virtual list viewports and their realized rows.
    pub lists: Vec<(NodeId, crate::list::List)>,
    pub decorations: HashMap<NodeId, Decoration>,
    /// Application values living exactly as long as their control.
    pub kept: HashMap<NodeId, Vec<Box<dyn std::any::Any>>>,
    #[cfg(feature = "motion")]
    pub motion: crate::motion::Motion,
    pub pending: VecDeque<(NodeId, u64)>,
    pub dispatching: bool,
    pub callback_version: u64,
    #[cfg(feature = "accessibility")]
    pub next_access_id: u64,
}

/// Default paragraph style for a theme.
pub(crate) fn text_style(theme: &Theme) -> TextStyle<'static> {
    TextStyle {
        size: theme.font_size,
        color: theme.foreground,
        ..Default::default()
    }
}

impl State {
    /// The theme resolved for a live node.
    pub fn theme_of(&self, id: NodeId) -> &Theme {
        self.tree.get(id).unwrap().context.theme_or(&self.theme)
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

    /// Inserts before child `position`; positions past the end append.
    pub fn insert(
        &mut self,
        parent: NodeId,
        position: usize,
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
        element.theme = self.tree.get(parent).unwrap().context.theme.clone();
        #[cfg(feature = "accessibility")]
        {
            element.access_id = aegle_access::accesskit::NodeId(self.next_access_id);
            self.next_access_id = self
                .next_access_id
                .checked_add(1)
                .ok_or(crate::UiError::IdentityExhausted)?;
        }
        let id = self
            .tree
            .insert_at(parent, position, LayoutNode::with_style(style, element))?;
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

impl State {
    /// Removes a non-root subtree, cancelling focus, capture and callbacks.
    pub fn remove_subtree(&mut self, id: NodeId) -> Result {
        self.cancel_subtree(id)?;
        self.tree.remove_with(id, |node, _| {
            self.callbacks.remove(&node);
            self.decorations.remove(&node);
            self.kept.remove(&node);
            self.lists.retain(|(list, _)| *list != node);
            #[cfg(feature = "motion")]
            {
                self.motion.tracks.remove(&node);
                self.motion.active.remove(&node);
                self.motion.moving.remove(&node);
                self.motion.ends.remove(&node);
            }
        })?;
        self.pending.retain(|(id, _)| self.tree.get(*id).is_some());
        self.invalidate_structure();
        Ok(())
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
