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
    pub mark: Mark,
    /// A check box shown as partially checked until the user changes it.
    pub mixed: bool,
}

pub(crate) use aegle_widgets::Mark;

/// Semantic and painting role of composite controls built from plain nodes.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Semantic {
    #[default]
    None,
    /// A button that opens a list of choices.
    Dropdown,
    /// A choice inside a dropdown list.
    Option,
    Table,
    TableRow,
    TableCell,
    TableHeader,
    /// An overlay shown above the window content.
    Popup,
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
    /// Presented scale and rotation (radians) about the bounds center, inherited
    /// by the subtree.
    pub spin: crate::Transform,
    /// Layout space to presented space for this node, set only inside a spun subtree.
    pub xf: Option<aegle_scene::Affine>,
    pub semantic: Semantic,
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
            spin: crate::Transform::default(),
            xf: None,
            semantic: Semantic::None,
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
    /// Shown or hidden popups with their anchors, in showing order.
    pub popups: Vec<crate::popup::PopupEntry>,
    /// Dropdown anchors and their choices.
    pub dropdowns: HashMap<NodeId, crate::popup::DropdownData>,
    pub decorations: HashMap<NodeId, Decoration>,
    /// Token overrides re-applied to the parent's theme whenever it changes.
    pub overrides: HashMap<NodeId, aegle_theme::ThemeOverride>,
    /// Fingers currently in contact.
    pub fingers: Vec<crate::touch::Finger>,
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

    /// Maps a window position into a node's layout space, undoing any ancestor spin.
    pub fn untransform(&self, id: NodeId, position: Point) -> Point {
        match self.tree.get(id).unwrap().context.xf.map(|xf| xf.inverse()) {
            Some(Ok(inverse)) => inverse.map_point(position),
            _ => position,
        }
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

    pub fn set_visible(&mut self, id: NodeId, visible: bool) -> Result {
        if self.tree.get(id).unwrap().context.visible == visible {
            return Ok(());
        }
        if !visible {
            self.cancel_subtree(id)?;
        }
        self.tree.get_mut(id).unwrap().context.visible = visible;
        let mut style = self.tree.get(id).unwrap().style().clone();
        style.display = if visible {
            aegle_layout::Display::Flex
        } else {
            aegle_layout::Display::None
        };
        aegle_layout::set_style(&mut self.tree, id, style)?;
        self.repaint = true;
        Ok(())
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
            self.overrides.remove(&node);
            self.kept.remove(&node);
            self.dropdowns.remove(&node);
            self.lists.retain(|(list, _)| *list != node);
            #[cfg(feature = "motion")]
            {
                self.motion.tracks.remove(&node);
                self.motion.active.remove(&node);
                self.motion.moving.remove(&node);
                self.motion.turning.remove(&node);
                self.motion.ends.remove(&node);
            }
        })?;
        self.pending.retain(|(id, _)| self.tree.get(*id).is_some());
        self.invalidate_structure();
        // Popups live under the root, apart from their anchors.
        self.prune_popups()
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
