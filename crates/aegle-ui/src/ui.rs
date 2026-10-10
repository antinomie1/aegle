use crate::OrFail;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    error::Error,
    fmt,
    rc::Rc,
};

use aegle_core::{Focus, NodeId, Tree};
use aegle_layout::{Dimension, Edges, FlexDirection, LayoutNode, LengthPercentage, Style};
use aegle_scene::Affine;
use aegle_text::{Selection, TextSystem};
use aegle_theme::Theme;
use aegle_types::{Color, Rect, Size};

use crate::{
    Container, Node,
    control::Plain,
    group::Visit,
    state::{Element, State},
};

/// Application operation or callback result. Underlying module errors are preserved.
pub type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// Errors specific to retained ownership and imperative operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UiError {
    /// The owning UI or node has been destroyed.
    DeadHandle,
    /// The requested operation is unavailable for this node type.
    WrongKind,
    /// Parent and child belong to different UIs.
    ForeignUi,
    /// A public numeric parameter is non-finite or outside its documented range.
    InvalidValue,
    /// The UI root cannot be removed or reparented.
    RootMutation,
    /// A handle or UI method was used while the UI was busy painting or
    /// running a hook: inside a `Control::paint`, a canvas painter, a `Hooks`
    /// function or a scene visitor. Do that work in a callback instead.
    ReentrantAccess,
    /// A monotonically increasing identity counter exhausted its range.
    IdentityExhausted,
    /// A token name is unregistered or registered with another type, or a
    /// handle names no registered token of its type.
    Token,
}
impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DeadHandle => "UI handle is no longer live",
            Self::WrongKind => "operation is not supported by this control",
            Self::ForeignUi => "nodes belong to different UIs",
            Self::InvalidValue => "UI value must be finite and within its documented range",
            Self::RootMutation => "UI root cannot be removed or reparented",
            Self::ReentrantAccess => {
                "UI handles cannot be used from a painter, hook or scene visitor; use a callback"
            }
            Self::IdentityExhausted => "UI identity counter exhausted",
            Self::Token => "token is unregistered or has another type",
        })
    }
}
impl Error for UiError {}

/// Bounded platform-neutral state for the focused editor's native IME session.
#[derive(Debug)]
pub struct ImeRequest {
    /// Contiguous excerpt, or no surrounding capability when selection exceeds budget.
    pub surrounding: Option<String>,
    /// UTF-8 byte endpoints relative to the excerpt, zero when absent.
    pub selection: Selection,
    /// Caret geometry in window logical coordinates, including editor scrolling.
    pub cursor_rect: Rect,
    /// Whether the focused editor accepts multiple lines.
    pub multiline: bool,
    /// The latest update came from the input method rather than application input.
    pub input_method: bool,
}
/// A pending native IME synchronization; send a disable first when `reset` is true.
#[derive(Debug)]
pub struct ImeState {
    /// Ends an old focus or composition session before publishing `request`.
    pub reset: bool,
    /// Current editable field; `None` disables native text input.
    pub request: Option<ImeRequest>,
}

/// Clipboard work requested by an editor shortcut; see [`Ui::take_clipboard`].
#[derive(Debug, PartialEq, Eq)]
pub enum ClipboardRequest {
    /// Store this text as the native clipboard selection.
    Write(String),
    /// Read native clipboard text and deliver it through [`Ui::paste`].
    Read,
}

/// A retained UI with shared text resources and no native platform dependency.
///
/// This owner is deliberately not `Clone`: handles hold weak references, so
/// dropping the UI destroys its controls even if application callbacks retain handles.
pub struct Ui {
    /// The shared engine state, for control libraries' hooks.
    pub state: Rc<RefCell<State>>,
    /// The root column, fixed for the UI's lifetime.
    root: NodeId,
}

impl Ui {
    /// Creates an empty root column. Fonts can be shared by all windows on this thread.
    pub fn with_fonts(fonts: Rc<RefCell<TextSystem>>, theme: Theme) -> Result<Self> {
        theme.validate()?;
        let mut tree = Tree::new();
        let mut element = Element::new(Box::new(Plain));
        #[cfg(feature = "accessibility")]
        {
            element.access_id = aegle_access::accesskit::NodeId(1);
        }
        #[cfg(not(feature = "accessibility"))]
        let _ = &mut element;
        let root = tree.insert(
            None,
            LayoutNode::with_style(container_style(&theme, true), element),
        )?;
        Ok(Self {
            state: Rc::new(RefCell::new(State {
                tree,
                root,
                order: vec![root],
                overlays: Vec::new(),
                topology_dirty: false,
                geometry_dirty: true,
                reveal_target: None,
                fonts,
                theme,
                size: Size::default(),
                focus: Focus::new(),
                last_focus: None,
                focus_visible: true,
                capture: None,
                drag: None,
                hover: None,
                pointer: None,
                ime_dirty: true,
                ime_reset: false,
                input_method: false,
                clipboard: None,
                repaint: true,
                callbacks: HashMap::new(),
                ext: HashMap::new(),
                hooks: Vec::new(),
                decorations: HashMap::new(),
                skins: HashMap::new(),
                decorators: HashMap::new(),
                overrides: HashMap::new(),
                tokens: Default::default(),
                fingers: Vec::new(),
                kept: HashMap::new(),
                #[cfg(feature = "motion")]
                motion: Default::default(),
                pending: VecDeque::new(),
                dispatching: false,
                callback_version: 0,
                frames: Vec::new(),
                key_handlers: Vec::new(),
                input_time: std::time::Instant::now(),
                clicks: Default::default(),
                drops: Default::default(),
                groups: Default::default(),
                frame_time: std::time::Instant::now(),
                animated: Default::default(),
                damage: Default::default(),
                damage_full: true,
                wake: None,
                descriptions: Default::default(),
                #[cfg(feature = "accessibility")]
                next_access_id: 2,
            })),
            root,
        })
    }

    /// Borrows the state to read it.
    ///
    /// # Panics
    ///
    /// While the UI is being changed, as when a control's paint calls back
    /// into it.
    pub(crate) fn read(&self) -> std::cell::Ref<'_, State> {
        let state = self.state.try_borrow();
        state.unwrap_or_else(|_| crate::handles::fail(UiError::ReentrantAccess.into()))
    }

    /// Borrows the state to change it; panics like [`Self::read`].
    pub(crate) fn write(&self) -> std::cell::RefMut<'_, State> {
        let state = self.state.try_borrow_mut();
        state.unwrap_or_else(|_| crate::handles::fail(UiError::ReentrantAccess.into()))
    }

    /// The root column; all public handles remain weak.
    pub fn root(&self) -> Container {
        Container(Node {
            state: Rc::downgrade(&self.state),
            id: self.root,
        })
    }

    /// The theme of the UI itself, which nodes without a local theme use.
    pub fn theme(&self) -> Theme {
        self.read().theme
    }

    /// Window clear color from the root's resolved theme.
    pub fn background(&self) -> Color {
        let state = self.read();
        state.theme_of(state.root).background
    }

    /// Changes the viewport's logical size. Zero is valid for a suspended surface.
    pub fn resize(&self, size: Size) {
        if ![size.width, size.height]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
        {
            panic!("{}", UiError::InvalidValue);
        }
        let mut state = self.write();
        if state.size == size {
            return;
        }
        state.size = size;
        let root = state.root;
        let mut style = state.tree.get(root).unwrap().style().clone();
        style.size = aegle_layout::Size {
            width: Dimension::length(size.width),
            height: Dimension::length(size.height),
        };
        aegle_layout::set_style(&mut state.tree, root, style).or_fail();
        state.damage_full = true;
        state.repaint = true;
        state.ime_dirty = true;
    }

    /// Updates virtual list rows, layout and only invalidated scene records.
    /// Returns whether pixels changed.
    pub fn refresh(&self) -> Result<bool> {
        let refresh = || self.write().refresh();
        self.realize()?;
        let mut repaint = refresh()?;
        // New layout can expose rows of resized or first-laid-out lists, and
        // measured content-sized rows can move the rows after them.
        for _ in 0..4 {
            let mut measured = false;
            {
                let mut state = self.write();
                for hook in state.hooks.clone() {
                    if let Some(measure) = hook.measure {
                        measured |= measure(&mut state)?;
                    }
                }
            }
            if !self.realize()? && !measured {
                break;
            }
            repaint |= refresh()?;
        }
        Ok(repaint)
    }

    /// Lets installed control libraries build or drop virtual content, outside any
    /// engine borrow. Returns whether anything changed.
    fn realize(&self) -> Result<bool> {
        let hooks = self.read().hooks.clone();
        let mut changed = false;
        for hook in hooks {
            if let Some(realize) = hook.realize {
                changed |= realize(self)?;
            }
        }
        Ok(changed)
    }

    /// Visits what to draw in order: visible records with their window-space
    /// placement and ancestor clip, and the layers of subtrees with a group
    /// opacity or backdrop blur around their records. Hosts must apply every
    /// clip and layer, mapped by their device scale; a renderer that cannot
    /// draw a layer must fail rather than skip it. Call after refresh; the
    /// callback must not mutate this UI.
    pub fn visit_scenes(&self, mut visit: impl FnMut(Visit<'_>) -> Result) -> Result {
        let state = self.read();
        // Scroll bars overlay their viewport's entire subtree.
        let mut overlays = state.overlays.iter().peekable();
        let draw = |id, overlay: bool, visit: &mut dyn FnMut(Visit<'_>) -> Result| -> Result {
            let element = &state.tree.get(id).unwrap().context;
            let scene = match &element.overlay {
                Some(scene) if overlay => scene,
                _ => &element.scene,
            };
            let shown = element.xf.map_or(element.bounds, |xf| {
                crate::scroll::map_rect(xf, element.bounds)
            });
            if element.effective_visible
                && element
                    .clip
                    .is_none_or(|clip| clip.intersection(shown).is_some())
                && !scene.commands().is_empty()
            {
                let place = Affine::translation(element.bounds.origin.x, element.bounds.origin.y);
                visit(Visit::Scene {
                    scene,
                    transform: element.xf.map_or(Ok(place), |xf| place.then(xf))?,
                    clip: element.clip,
                })?;
            }
            Ok(())
        };
        // Open layers by node; a node's layer closes when the order leaves
        // its subtree. A transparent subtree is skipped entirely.
        let mut open: Vec<NodeId> = Vec::new();
        let mut hidden: Option<NodeId> = None;
        let close = |open: &mut Vec<NodeId>, node, visit: &mut dyn FnMut(Visit<'_>) -> Result| {
            while let Some(&layer) = open.last()
                && !state.contains(layer, node)
            {
                open.pop();
                visit(Visit::PopLayer)?;
            }
            Ok::<_, Box<dyn Error>>(())
        };
        for (index, &id) in state.order.iter().enumerate() {
            while let Some(&(_, view)) = overlays.next_if(|&&(end, _)| end <= index) {
                close(&mut open, view, &mut visit)?;
                if hidden.is_none_or(|h| !state.contains(h, view)) {
                    draw(view, true, &mut visit)?;
                }
            }
            close(&mut open, id, &mut visit)?;
            if let Some(h) = hidden {
                if state.contains(h, id) {
                    continue;
                }
                hidden = None;
            }
            let group = state.tree.get(id).unwrap().context.group;
            if group.layered() && group.opacity == 0.0 {
                hidden = Some(id);
                continue;
            }
            if let Some(layer) = state.layer(id)? {
                visit(Visit::PushLayer(layer))?;
                open.push(id);
            }
            draw(id, false, &mut visit)?;
        }
        for &(_, view) in overlays {
            close(&mut open, view, &mut visit)?;
            if hidden.is_none_or(|h| !state.contains(h, view)) {
                draw(view, true, &mut visit)?;
            }
        }
        for _ in open {
            visit(Visit::PopLayer)?;
        }
        Ok(())
    }

    /// Consumes the latest copy, cut or paste request. Password fields never
    /// request a write; cut has already deleted its selection.
    pub fn take_clipboard(&self) -> Option<ClipboardRequest> {
        self.write().clipboard.take()
    }

    /// Consumes pending IME synchronization after refresh and event callbacks.
    pub fn take_ime_state(&self, max_bytes: usize) -> Option<ImeState> {
        let mut state = self.write();
        if !state.ime_dirty {
            return None;
        }
        state.ime_dirty = false;
        let reset = std::mem::take(&mut state.ime_reset);
        let request = state.focus.current(&state.tree).and_then(|id| {
            let element = &state.tree.get(id).unwrap().context;
            let field = element.control.editor()?;
            if !field.accepts_ime() {
                return None;
            }
            let surrounding = field.editor().surrounding(max_bytes);
            let selection = surrounding.map(|s| s.selection).unwrap_or_default();
            let mut cursor_rect = field.editor().ime_rect();
            let origin = element.text_origin(&state.theme);
            cursor_rect.origin.x += element.bounds.origin.x + origin.x;
            cursor_rect.origin.y += element.bounds.origin.y + origin.y;
            // Keep a manually scrolled-out composition alive. Its candidate
            // anchor collapses at the nearest visible edge until it re-enters.
            if let Some(xf) = element.xf {
                cursor_rect = crate::scroll::map_rect(xf, cursor_rect);
            }
            cursor_rect = crate::scroll_geometry::clamp_anchor(cursor_rect, element.bounds);
            if let Some(clip) = element.clip {
                cursor_rect = crate::scroll_geometry::clamp_anchor(cursor_rect, clip);
            }
            cursor_rect = crate::scroll_geometry::clamp_anchor(
                cursor_rect,
                Rect::new(0.0, 0.0, state.size.width, state.size.height),
            );
            Some(ImeRequest {
                surrounding: surrounding.map(|s| s.to_string()),
                selection,
                cursor_rect,
                multiline: field.editor().is_multiline(),
                input_method: state.input_method,
            })
        });
        Some(ImeState { reset, request })
    }

    /// Destroys the whole tree, cancelling focus, capture, callbacks and
    /// animations; every handle becomes dead. A host calls this when its window
    /// closes.
    pub fn close(&self) {
        let mut state = self.write();
        let root = state.root;
        state.tree.remove(root).or_fail();
        state.order.clear();
        state.pending.clear();
        state.callbacks.clear();
        state.decorations.clear();
        state.overrides.clear();
        state.fingers.clear();
        state.kept.clear();
        state.ext.clear();
        #[cfg(feature = "motion")]
        state.motion.clear();
        state.capture = None;
        state.drag = None;
        state.hover = None;
        state.pointer = None;
        state.reveal_target = None;
    }
}

/// Scroll views keep content clear of their default border.
pub fn scroll_padding(theme: &Theme) -> Edges<LengthPercentage> {
    let p = LengthPercentage::length(theme.padding / 2.0);
    Edges {
        left: p,
        right: p,
        top: p,
        bottom: p,
    }
}

/// The default layout style of a plain row or column under `theme`.
pub fn container_style(theme: &Theme, root: bool) -> Style {
    let gap = LengthPercentage::length(theme.gap);
    let padding = LengthPercentage::length(if root { theme.padding } else { 0.0 });
    Style {
        flex_direction: FlexDirection::Column,
        gap: aegle_layout::Size {
            width: gap,
            height: gap,
        },
        padding: Edges {
            left: padding,
            right: padding,
            top: padding,
            bottom: padding,
        },
        ..Default::default()
    }
}
