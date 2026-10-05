use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    error::Error,
    fmt,
    rc::Rc,
};

use aegle_core::{Focus, Route, Tree};
use aegle_layout::{Dimension, Edges, FlexDirection, LayoutNode, LengthPercentage, Style};
use aegle_scene::{Affine, Scene};
use aegle_text::{Selection, TextSystem};
use aegle_theme::Theme;
use aegle_types::{Color, Rect, Size};

use crate::{
    Container, Node,
    state::{Content, Element, State},
};

/// Application operation or callback result. Underlying module errors are preserved.
pub type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// Errors specific to retained ownership and imperative operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// A scene visitor tried to modify its currently borrowed UI.
    ReentrantAccess,
    /// A monotonically increasing identity counter exhausted its range.
    IdentityExhausted,
}
impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DeadHandle => "UI handle is no longer live",
            Self::WrongKind => "operation is not supported by this control",
            Self::ForeignUi => "nodes belong to different UIs",
            Self::InvalidValue => "UI value must be finite and within its documented range",
            Self::RootMutation => "UI root cannot be removed or reparented",
            Self::ReentrantAccess => "UI state is already borrowed by a visitor",
            Self::IdentityExhausted => "UI identity counter exhausted",
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
    pub(crate) state: Rc<RefCell<State>>,
}

impl Ui {
    /// Creates an empty root column. Fonts can be shared by all windows on this thread.
    pub fn with_fonts(fonts: Rc<RefCell<TextSystem>>, theme: Theme) -> Result<Self> {
        theme.validate()?;
        let mut tree = Tree::new();
        let mut element = Element::new(Content::Container);
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
                route: Route::new(),
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
                lists: Vec::new(),
                popups: Vec::new(),
                dropdowns: HashMap::new(),
                decorations: HashMap::new(),
                kept: HashMap::new(),
                #[cfg(feature = "motion")]
                motion: Default::default(),
                pending: VecDeque::new(),
                dispatching: false,
                callback_version: 0,
                #[cfg(feature = "accessibility")]
                next_access_id: 2,
            })),
        })
    }

    /// The root column; all public handles remain weak.
    pub fn root(&self) -> Container {
        Container(Node {
            state: Rc::downgrade(&self.state),
            id: self.state.borrow().root,
        })
    }

    /// Window clear color from the root's resolved theme.
    pub fn background(&self) -> Color {
        let state = self.state.borrow();
        state.theme_of(state.root).background
    }

    /// Changes the viewport's logical size. Zero is valid for a suspended surface.
    pub fn resize(&self, size: Size) -> Result {
        if ![size.width, size.height]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
        {
            return Err(UiError::InvalidValue.into());
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if state.size == size {
            return Ok(());
        }
        state.size = size;
        let root = state.root;
        let mut style = state.tree.get(root).unwrap().style().clone();
        style.size = aegle_layout::Size {
            width: Dimension::length(size.width),
            height: Dimension::length(size.height),
        };
        aegle_layout::set_style(&mut state.tree, root, style)?;
        state.repaint = true;
        state.ime_dirty = true;
        Ok(())
    }

    /// Updates virtual list rows, layout and only invalidated scene records.
    /// Returns whether pixels changed.
    pub fn refresh(&self) -> Result<bool> {
        let refresh = || {
            self.state
                .try_borrow_mut()
                .map_err(|_| UiError::ReentrantAccess)?
                .refresh()
        };
        self.realize_rows()?;
        let mut repaint = refresh()?;
        // New layout can expose rows of resized or first-laid-out lists, and
        // measured content-sized rows can move the rows after them.
        for _ in 0..4 {
            let measured = self
                .state
                .try_borrow_mut()
                .map_err(|_| UiError::ReentrantAccess)?
                .measure_rows()?;
            if !self.realize_rows()? && !measured {
                break;
            }
            repaint |= refresh()?;
        }
        Ok(repaint)
    }

    /// Visits visible records with their window-space translation and ancestor clip.
    /// The optional clip is in logical window coordinates, outside the translation.
    /// Hosts must apply it (and their device scale) to preserve scroll clipping.
    /// Call after refresh; the callback must not mutate this UI.
    pub fn visit_scenes(
        &self,
        mut visit: impl FnMut(&Scene, Affine, Option<Rect>) -> Result,
    ) -> Result {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| UiError::ReentrantAccess)?;
        // Scroll bars overlay their viewport's entire subtree.
        let mut overlays = state.overlays.iter().peekable();
        let mut draw = |id, overlay: bool| -> Result {
            let element = &state.tree.get(id).unwrap().context;
            let scene = match &element.content {
                Content::Scroll(scene) if overlay => scene,
                _ => &element.scene,
            };
            if element.effective_visible
                && element
                    .clip
                    .is_none_or(|clip| clip.intersection(element.bounds).is_some())
                && !scene.commands().is_empty()
            {
                visit(
                    scene,
                    Affine::translation(element.bounds.origin.x, element.bounds.origin.y)?,
                    element.clip,
                )?;
            }
            Ok(())
        };
        for (index, &id) in state.order.iter().enumerate() {
            while let Some(&(_, view)) = overlays.next_if(|&&(end, _)| end <= index) {
                draw(view, true)?;
            }
            draw(id, false)?;
        }
        for &(_, view) in overlays {
            draw(view, true)?;
        }
        Ok(())
    }

    /// Consumes the latest copy, cut or paste request. Password fields never
    /// request a write; cut has already deleted its selection.
    pub fn take_clipboard(&self) -> Result<Option<ClipboardRequest>> {
        Ok(self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .clipboard
            .take())
    }

    /// Consumes pending IME synchronization after refresh and event callbacks.
    pub fn take_ime_state(&self, max_bytes: usize) -> Result<Option<ImeState>> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if !state.ime_dirty {
            return Ok(None);
        }
        state.ime_dirty = false;
        let reset = std::mem::take(&mut state.ime_reset);
        let request = state.focus.current(&state.tree).and_then(|id| {
            let element = &state.tree.get(id).unwrap().context;
            let Content::Field(field) = &element.content else {
                return None;
            };
            if !field.accepts_ime() {
                return None;
            }
            let surrounding = field.editor().surrounding(max_bytes);
            let selection = surrounding.map(|s| s.selection).unwrap_or_default();
            let mut cursor_rect = field.editor().ime_rect();
            let padding = element
                .padding
                .unwrap_or(element.theme_or(&state.theme).padding);
            cursor_rect.origin.x += element.bounds.origin.x + padding - element.scroll.x;
            cursor_rect.origin.y += element.bounds.origin.y + padding - element.scroll.y;
            // Keep a manually scrolled-out composition alive. Its candidate
            // anchor collapses at the nearest visible edge until it re-enters.
            cursor_rect = crate::scroll::clamp_anchor(cursor_rect, element.bounds);
            if let Some(clip) = element.clip {
                cursor_rect = crate::scroll::clamp_anchor(cursor_rect, clip);
            }
            cursor_rect = crate::scroll::clamp_anchor(
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
        Ok(Some(ImeState { reset, request }))
    }

    #[cfg(any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    pub(crate) fn close(&self) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        let root = state.root;
        state.tree.remove(root)?;
        state.order.clear();
        state.pending.clear();
        state.callbacks.clear();
        state.decorations.clear();
        state.kept.clear();
        state.lists.clear();
        state.popups.clear();
        state.dropdowns.clear();
        #[cfg(feature = "motion")]
        {
            state.motion.tracks.clear();
            state.motion.active.clear();
            state.motion.moving.clear();
            state.motion.ends.clear();
        }
        state.capture = None;
        state.drag = None;
        state.hover = None;
        state.pointer = None;
        state.reveal_target = None;
        Ok(())
    }
}

/// Scroll views keep content clear of their default border.
pub(crate) fn scroll_padding(theme: &Theme) -> Edges<LengthPercentage> {
    let p = LengthPercentage::length(theme.padding / 2.0);
    Edges {
        left: p,
        right: p,
        top: p,
        bottom: p,
    }
}

pub(crate) fn container_style(theme: &Theme, root: bool) -> Style {
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
