// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.

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
    /// The control implementation; it keeps its own state.
    pub control: Box<dyn Control>,
    /// The control's own painted records, in local coordinates.
    pub scene: Scene,
    /// What a viewport draws over its children, when the control is one.
    pub overlay: Option<Box<Scene>>,
    /// Layout rectangle in the parent's space.
    pub bounds: Rect,
    /// Clip inherited from scrolling ancestors, in window space.
    pub clip: Option<Rect>,
    /// Clips its children's painting and hit testing to its bounds.
    pub clips: bool,
    /// Current scroll offset of a viewport.
    pub scroll: Point,
    /// Whether the node itself is shown.
    pub visible: bool,
    /// Layout mode to restore when a hidden node is shown again.
    pub shown_display: aegle_layout::Display,
    /// Whether the node and every ancestor are shown.
    pub effective_visible: bool,
    /// Whether the node accepts input.
    pub enabled: bool,
    /// Accessible label.
    pub label: String,
    /// Scroll the caret into view on the next refresh.
    pub ensure_caret: bool,
    /// Layout properties set on this node, preserved across theme changes.
    pub local_layout: crate::LocalLayout,
    /// Local content padding; `None` uses the control's default.
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
    /// The skin set on this node or inherited for its kind; `None` uses the
    /// kind's default.
    pub skin: Option<aegle_theme::Skin>,
    /// Presented group opacity and backdrop blur of the subtree.
    pub group: crate::group::Group,
    /// Nearest local theme of this node or an ancestor; `None` uses the UI theme.
    pub theme: Option<Rc<Theme>>,
    /// Whether `theme` was set on this node rather than inherited.
    pub local_theme: bool,
    /// Layout direction set on this node; `None` inherits the parent's. The
    /// resolved direction is the layout style's `direction`.
    pub direction: Option<aegle_layout::LayoutDirection>,
    /// The resolved direction is right to left.
    pub rtl: bool,
    /// Accessibility id of this node.
    #[cfg(feature = "accessibility")]
    pub access_id: aegle_access::accesskit::NodeId,
}

impl Element {
    /// The local theme of this node, else `ui`.
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
    /// A visible, enabled element for `control` with no local settings.
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
            skin: None,
            group: crate::group::Group::NONE,
            theme: None,
            local_theme: false,
            direction: None,
            rtl: false,
            #[cfg(feature = "accessibility")]
            access_id: aegle_access::accesskit::NodeId(0),
        }
    }
}

/// The engine state behind a [`crate::Ui`], also the authoring surface for control
/// libraries: fields and methods are public so a control can use the tree, focus,
/// themes and invalidation directly. Prefer the typed handles in application code.
pub struct State {
    /// The retained tree of nodes with their layout.
    pub tree: Tree<LayoutNode<Element>>,
    /// The root node.
    pub root: NodeId,
    /// Nodes in paint order.
    pub order: Vec<NodeId>,
    /// Scroll views in post-order with the end of their subtree in `order`.
    pub overlays: Vec<(usize, NodeId)>,
    /// The tree structure changed since the last refresh.
    pub topology_dirty: bool,
    /// Layout must be recomputed.
    pub geometry_dirty: bool,
    /// Node to scroll into view on the next refresh.
    pub reveal_target: Option<NodeId>,
    /// Shared text system.
    pub fonts: Rc<RefCell<TextSystem>>,
    /// The UI theme (fields on `Element` hold local themes).
    pub theme: Theme,
    /// Window size in logical pixels.
    pub size: Size,
    /// Logical focus state.
    pub focus: Focus,
    /// Last focused node, restored when focus returns to the window.
    pub last_focus: Option<NodeId>,
    /// Pointer route of the press in progress.
    pub route: Route,
    /// Pointer capture: the pointer and its target.
    pub capture: Option<(PointerId, NodeId)>,
    /// Scrollbar drag in progress.
    pub drag: Option<crate::scrollbar::Drag>,
    /// Node under the pointer.
    pub hover: Option<NodeId>,
    /// Last pointer and its window position.
    pub pointer: Option<(PointerId, Point)>,
    /// The input-method state changed and must be re-reported.
    pub ime_dirty: bool,
    /// The input method must drop its current composition.
    pub ime_reset: bool,
    /// An input method is active for the focused editor.
    pub input_method: bool,
    /// Pending host clipboard request.
    pub clipboard: Option<crate::ClipboardRequest>,
    /// A repaint was requested.
    pub repaint: bool,
    /// Event callbacks by node.
    pub callbacks: HashMap<NodeId, Handler>,
    /// Per-library data keyed by type, see [`State::ext`].
    pub ext: HashMap<std::any::TypeId, Box<dyn std::any::Any>>,
    /// Installed control-library hooks, see [`crate::Hooks`].
    pub hooks: Vec<&'static crate::Hooks>,
    /// Visual decoration per node.
    pub decorations: HashMap<NodeId, Decoration>,
    /// Skins set on nodes, for the node itself or a kind in its subtree.
    pub skins: HashMap<NodeId, Vec<(Option<&'static aegle_theme::ControlKind>, aegle_theme::Skin)>>,
    /// Token overrides re-applied to the parent's theme whenever it changes.
    pub overrides: HashMap<NodeId, aegle_theme::ThemeOverride>,
    /// Custom token overrides and token bindings.
    pub tokens: crate::Tokens,
    /// Fingers currently in contact.
    pub fingers: Vec<crate::touch::Finger>,
    /// Application values living exactly as long as their control.
    pub kept: HashMap<NodeId, Vec<Box<dyn std::any::Any>>>,
    /// Transition tracks and the animation clock.
    #[cfg(feature = "motion")]
    pub motion: crate::motion::Motion,
    /// Callbacks queued to run outside the engine borrow.
    pub pending: VecDeque<(NodeId, u64)>,
    /// A callback queue is being drained.
    pub dispatching: bool,
    /// Bumped when callbacks are replaced during dispatch.
    pub callback_version: u64,
    /// Per-frame callbacks in registration order, see [`crate::Node::on_frame`].
    pub frames: Vec<crate::events::FrameHandler>,
    /// The window key handler, see [`crate::Ui::on_key`].
    pub key_handler: Option<crate::events::KeyHandler>,
    /// Replacement count of the key handler, so one installed during a call survives.
    pub key_version: u64,
    /// When the input being dispatched was reported.
    pub input_time: std::time::Instant,
    /// Press counting and double-click handlers.
    pub clicks: crate::clicks::Clicks,
    /// Nodes with a group effect and what their layer last drew.
    pub groups: HashMap<NodeId, crate::group::Drawn>,
    /// The time of the frame being produced, see [`crate::Ui::run_frame`].
    pub frame_time: std::time::Instant,
    /// Controls that asked to repaint on the next frame.
    pub animated: std::collections::HashSet<NodeId>,
    /// Changed area not yet presented, see [`crate::Ui::damage`].
    pub damage: aegle_types::Region<aegle_types::Rect>,
    /// The whole window changed since the last present.
    pub damage_full: bool,
    /// When a control library wants its [`crate::Hooks::wake`] called; see [`crate::Ui::next_wake`].
    pub wake: Option<std::time::Instant>,
    /// Accessible descriptions, see [`crate::Node::set_accessible_description`].
    pub descriptions: HashMap<NodeId, String>,
    /// Next accessibility node id to hand out.
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

    /// Recomputes paint order after a structure change.
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

    /// Whether `node` is `root` or inside its subtree.
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

    /// Whether the node and all its ancestors are visible and enabled.
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

    /// Shows or hides a node, restoring its previous layout mode when shown.
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

    /// Marks order, damage and paint stale after nodes were added, moved or removed.
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
        let editor = control
            .kind()
            .accepts
            .contains(aegle_theme::Accepts::EDITOR);
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
        self.resolve_skin(id)?;
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
            self.clicks.handlers.remove(&node);
            self.clicks.menus.remove(&node);
            self.frames.retain(|h| h.id != node);
            self.animated.remove(&node);
            self.descriptions.remove(&node);
            self.decorations.remove(&node);
            self.skins.remove(&node);
            self.overrides.remove(&node);
            self.tokens.forget(node);
            self.kept.remove(&node);
            #[cfg(feature = "motion")]
            self.motion.forget(node);
            self.groups.remove(&node);
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
    pub fn install(&mut self, hooks: &'static crate::Hooks) {
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
        let control: &mut dyn std::any::Any = &mut *self.tree.get_mut(id)?.context.control;
        control.downcast_mut()
    }
}

/// Focus traversal policy: hidden or disabled nodes prune their subtree.
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
