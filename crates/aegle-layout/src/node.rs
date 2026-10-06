use aegle_types::Rect;
use taffy::{Cache, Layout, Style};

use std::rc::Rc;

thread_local! {
    static DEFAULT_STYLE: Rc<Style> = Rc::new(Style::DEFAULT);
}

/// Layout state and caller-owned measurement data for one retained node.
///
/// Styles identical to Taffy's default share one thread-local object. The context can
/// contain a text resource ID, an intrinsic image size, or the caller's own data.
pub struct LayoutNode<C> {
    pub(crate) style: Rc<Style>,
    pub(crate) cache: Cache,
    pub(crate) layout: Layout,
    /// Transparent: children take part in the nearest non-contents ancestor's layout.
    pub(crate) contents: bool,
    /// Host-owned measurement or control data.
    pub context: C,
}

impl<C> LayoutNode<C> {
    /// Creates a node with shared default style.
    pub fn new(context: C) -> Self {
        Self {
            style: DEFAULT_STYLE.with(Rc::clone),
            cache: Cache::new(),
            layout: Layout::with_order(0),
            contents: false,
            context,
        }
    }

    /// Creates a node with explicit style.
    pub fn with_style(style: Style, context: C) -> Self {
        let mut node = Self::new(context);
        node.set_style(style);
        node
    }

    /// The node's layout style.
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// Whether this node is transparent to layout; see [`crate::set_contents`].
    pub fn is_contents(&self) -> bool {
        self.contents
    }

    /// Computed bounds relative to the parent's content coordinate space.
    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.layout.location.x,
            self.layout.location.y,
            self.layout.size.width,
            self.layout.size.height,
        )
    }

    /// Complete layout result, including borders, padding and content size.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub(crate) fn set_style(&mut self, style: Style) {
        self.style = if style == Style::DEFAULT {
            DEFAULT_STYLE.with(Rc::clone)
        } else {
            Rc::new(style)
        };
    }
}
