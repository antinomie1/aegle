// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.
#![allow(missing_docs)]

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use aegle_controls::PointerId;
use aegle_core::{Focus, NodeId, Route, Tree};
use aegle_layout::LayoutNode;
use aegle_scene::Scene;
use aegle_text::{TextStyle, TextSystem};
use aegle_theme::Theme;
use aegle_types::{Point, Rect, Size};

use crate::{Result, callbacks::Handler, control::Control, style::Decoration};

/// A node's engine-side data. Controls keep their own state in `control`.
pub struct Element {
    pub control: Box<dyn Control>,
    pub scene: Scene,
    /// What a viewport draws over its children, when the control is one.
    pub overlay: Option<Box<Scene>>,
    pub bounds: Rect,
    pub clip: Option<Rect>,
    /// Clips its children's painting and hit testing to its bounds.
    pub clips: bool,
    pub scroll: Point,
    pub visible: bool,
    /// Layout mode to restore when a hidden node is shown again.
    pub shown_display: aegle_layout::Display,
    pub effective_visible: bool,
    pub enabled: bool,
    pub label: String,
    pub ensure_caret: bool,
    pub local_layout: crate::LocalLayout,
    pub padding: Option<f32>,
    /// Presented translation after layout, inherited by the subtree.
    pub offset: Point,
    /// Presented scale and rotation (radians) about the bounds center, inherited
    /// by the subtree.
    pub spin: crate::Transform,
    /// Layout space to presented space for this node, set only inside a spun subtree.
    pub xf: Option<aegle_scene::Affine>,
    /// Window area this node's records covered when last refreshed.
    pub painted: Option<Rect>,
    /// Nearest local theme of this node or an ancestor; `None` uses the UI theme.
    pub theme: Option<Rc<Theme>>,
    /// Whether `theme` was set on this node rather than inherited.
    pub local_theme: bool,
    /// Layout direction set on this node; `None` inherits the parent's. The
    /// resolved direction is the layout style's `direction`.
    pub direction: Option<aegle_layout::LayoutDirection>,
    /// The resolved direction is right to left.
    pub rtl: bool,
    #[cfg(feature = "accessibility")]
    pub access_id: aegle_access::accesskit::NodeId,
}

impl Element {
    pub fn theme_or<'a>(&'a self, ui: &'a Theme) -> &'a Theme {
        self.theme.as_deref().unwrap_or(ui)
    }
    /// Content padding: the local value, else the control's default.
    pub fn inset(&self, ui: &Theme) -> f32 {
        self.padding
            .unwrap_or_else(|| self.control.default_padding(self.theme_or(ui)))
    }
    /// Where an editor's scrolled text starts in its local coordinates.
    pub fn text_origin(&self, ui: &Theme) -> Point {
        let shift = self
            .control
            .content_offset(self.bounds.size, self.inset(ui), self.scroll);
        Point::new(-shift.x, -shift.y)
    }
    pub fn new(control: Box<dyn Control>) -> Self {
        let overlay = control.viewport().then(Box::default);
        Self {
            control,
            scene: Scene::default(),
            overlay,
            bounds: Rect::default(),
            clip: None,
            clips: false,
            scroll: Point::default(),
            visible: true,
            shown_display: aegle_layout::Display::Flex,
            effective_visible: true,
            enabled: true,
            label: String::new(),
            ensure_caret: false,
            local_layout: crate::LocalLayout::NONE,
            padding: None,
            offset: Point::default(),
            spin: crate::Transform::default(),
            xf: None,
            painted: None,
            theme: None,
            local_theme: false,
            direction: None,
            rtl: false,
            #[cfg(feature = "accessibility")]
            access_id: aegle_access::accesskit::NodeId(0),
        }
    }
}

/// Tree-wide behavior a control library adds to the engine, as plain function
/// pointers so a hook can run while the engine is borrowed and call back into it.
/// Install with [`State::install`]; every field is optional.
#[derive(Default)]
pub struct Hooks {
    /// A key press before focus traversal; returns whether it was used.
    pub key: Option<fn(&mut State, &aegle_controls::KeyInput<'_>) -> Result<bool>>,
    /// A primary press at a window point, before it is routed.
    pub press: Option<fn(&mut State, Point) -> Result>,
    /// The overlay node covering a window point, which blocks hits below it.
    pub overlay_at: Option<fn(&State, Point) -> Option<NodeId>>,
    /// After geometry: moves overlays; returns whether anything moved.
    pub place: Option<fn(&mut State) -> bool>,
    /// A node was removed (called for it and each descendant, children first, after the
    /// subtree is destroyed). The id is dead: use it only as the key of library data
    /// and never query the tree with it.
    pub removed: Option<fn(&mut State, NodeId)>,
    /// A subtree was removed.
    pub removed_after: Option<fn(&mut State) -> Result>,
    /// After layout: measures realized content; returns whether anything moved.
    pub measure: Option<fn(&mut State) -> Result<bool>>,
    /// Before refresh, outside any engine borrow: builds or drops virtual content.
    pub realize: Option<fn(&crate::Ui) -> Result<bool>>,
    /// The hovered control changed (to `None` when the pointer left).
    pub hover: Option<fn(&mut State, Option<NodeId>) -> Result>,
    /// [`State::wake`] passed: delayed work such as showing a tooltip.
    pub wake: Option<fn(&mut State, std::time::Instant) -> Result>,
}

/// The engine state behind a [`crate::Ui`], also the authoring surface for control
/// libraries: fields and methods are public so a control can use the tree, focus,
/// themes and invalidation directly. Prefer the typed handles in application code.
pub struct State {
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
    /// Per-library data keyed by type, see [`State::ext`].
    pub ext: HashMap<std::any::TypeId, Box<dyn std::any::Any>>,
    /// Installed control-library hooks, see [`Hooks`].
    pub hooks: Vec<&'static Hooks>,
    pub decorations: HashMap<NodeId, Decoration>,
    /// Token overrides re-applied to the parent's theme whenever it changes.
    pub overrides: HashMap<NodeId, aegle_theme::ThemeOverride>,
    /// Custom token overrides and token bindings.
    pub tokens: crate::Tokens,
    /// Fingers currently in contact.
    pub fingers: Vec<crate::touch::Finger>,
    /// Application values living exactly as long as their control.
    pub kept: HashMap<NodeId, Vec<Box<dyn std::any::Any>>>,
    #[cfg(feature = "motion")]
    pub motion: crate::motion::Motion,
    pub pending: VecDeque<(NodeId, u64)>,
    pub dispatching: bool,
    pub callback_version: u64,
    /// Per-frame callbacks in registration order, see [`crate::Node::on_frame`].
    pub frames: Vec<crate::events::FrameHandler>,
    /// The window key handler, see [`crate::Ui::on_key`].
    pub key_handler: Option<crate::events::KeyHandler>,
    /// Replacement count of the key handler, so one installed during a call survives.
    pub key_version: u64,
    /// When the input being dispatched was reported.
    pub input_time: std::time::Instant,
    /// The time of the frame being produced, see [`crate::Ui::run_frame`].
    pub frame_time: std::time::Instant,
    /// Controls that asked to repaint on the next frame.
    pub animated: std::collections::HashSet<NodeId>,
    /// Changed area not yet presented, see [`crate::Ui::damage`].
    pub damage: aegle_types::Region<aegle_types::Rect>,
    /// The whole window changed since the last present.
    pub damage_full: bool,
    /// When a control library wants its [`Hooks::wake`] called; see [`crate::Ui::next_wake`].
    pub wake: Option<std::time::Instant>,
    /// Accessible descriptions, see [`crate::Node::set_accessible_description`].
    pub descriptions: HashMap<NodeId, String>,
    #[cfg(feature = "accessibility")]
    pub next_access_id: u64,
}

/// Default paragraph style for a theme.
pub fn text_style(theme: &Theme) -> TextStyle<'static> {
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
                if self.tree.get(id).unwrap().context.control.viewport() {
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
        if visible {
            style.display = self.tree.get(id).unwrap().context.shown_display;
        } else {
            self.tree.get_mut(id).unwrap().context.shown_display = style.display;
            style.display = aegle_layout::Display::None;
        }
        aegle_layout::set_style(&mut self.tree, id, style)?;
        self.repaint = true;
        Ok(())
    }

    pub fn invalidate_structure(&mut self) {
        self.topology_dirty = true;
        // Drawing order changed; removed nodes no longer report their area.
        self.damage_full = true;
        self.repaint = true;
        self.ime_dirty = true;
    }

    /// Inserts before child `position`; positions past the end append.
    pub fn insert(
        &mut self,
        parent: NodeId,
        position: usize,
        control: Box<dyn Control>,
        mut style: aegle_layout::Style,
    ) -> Result<NodeId> {
        let editor = control.kind() == aegle_theme::ControlKind::TextField;
        if editor != control.editor().is_some() {
            return Err(crate::UiError::WrongKind.into());
        }
        // Self-clipping controls manage any text scrolling internally; their
        // intrinsic overflow must not enlarge an ancestor view.
        if control.self_clipping() {
            style.overflow.x = aegle_layout::Overflow::Hidden;
            style.overflow.y = aegle_layout::Overflow::Hidden;
        }
        #[cfg(feature = "grid")]
        crate::grid_handles::stack_child(self, parent, &mut style);
        style.direction = self.tree.get(parent).unwrap().style().direction;
        let mut element = Element::new(control);
        element.theme = self.tree.get(parent).unwrap().context.theme.clone();
        element.rtl = self.tree.get(parent).unwrap().context.rtl;
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
        if self.tree.get(id).unwrap().context.control.interactive()
            && let Some(timing) = self.motion.default
        {
            self.motion
                .tracks
                .insert(id, crate::motion::Track::uniform(timing));
        }
        self.invalidate_structure();
        Ok(id)
    }
}

impl State {
    /// Removes a non-root subtree, cancelling focus, capture and callbacks.
    pub fn remove_subtree(&mut self, id: NodeId) -> Result {
        self.cancel_subtree(id)?;
        let mut removed = Vec::new();
        self.tree.remove_with(id, |node, _| {
            removed.push(node);
            self.callbacks.remove(&node);
            self.frames.retain(|h| h.id != node);
            self.animated.remove(&node);
            self.descriptions.remove(&node);
            self.decorations.remove(&node);
            self.overrides.remove(&node);
            self.tokens.forget(node);
            self.kept.remove(&node);
            #[cfg(feature = "motion")]
            self.motion.forget(node);
        })?;
        self.pending.retain(|(id, _)| self.tree.get(*id).is_some());
        self.invalidate_structure();
        for hook in self.hooks.clone() {
            if let Some(removed_node) = hook.removed {
                removed.iter().for_each(|&node| removed_node(self, node));
            }
        }
        // Library overlays may live apart from the removed node's subtree.
        for hook in self.hooks.clone() {
            if let Some(after) = hook.removed_after {
                after(self)?;
            }
        }
        Ok(())
    }

    /// Installs a control library's hooks once; later calls with the same set do nothing.
    pub fn install(&mut self, hooks: &'static Hooks) {
        if !self.hooks.iter().any(|h| std::ptr::eq(*h, hooks)) {
            self.hooks.push(hooks);
        }
    }

    /// The library data of type `T`, created with `Default` on first use.
    pub fn ext<T: std::any::Any + Default>(&mut self) -> &mut T {
        self.ext
            .entry(std::any::TypeId::of::<T>())
            .or_insert_with(|| Box::new(T::default()))
            .downcast_mut()
            .expect("keyed by type")
    }

    /// The library data of type `T`, if any was created.
    pub fn ext_ref<T: std::any::Any>(&self) -> Option<&T> {
        self.ext
            .get(&std::any::TypeId::of::<T>())
            .and_then(|data| data.downcast_ref())
    }

    /// The overlay node covering a window point, from the installed hooks.
    pub fn overlay_at(&self, position: Point) -> Option<NodeId> {
        self.hooks
            .iter()
            .find_map(|hook| hook.overlay_at.and_then(|at| at(self, position)))
    }

    /// The control of a live node as `C`, or `None` for another kind.
    pub fn control_as<C: Control>(&mut self, id: NodeId) -> Option<&mut C> {
        self.tree
            .get_mut(id)?
            .context
            .control
            .as_any_mut()
            .downcast_mut()
    }
}

pub fn focus_policy(_: NodeId, node: &LayoutNode<Element>) -> aegle_core::FocusPolicy {
    use aegle_core::FocusPolicy;
    if !node.context.visible || !node.context.enabled {
        return FocusPolicy::Prune;
    }
    if node.context.control.interactive() {
        FocusPolicy::Focusable
    } else {
        FocusPolicy::Skip
    }
}
