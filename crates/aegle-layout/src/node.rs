use aegle_types::{Rect, Size};
use taffy::{Cache, Layout};

use crate::LayoutStyle;

use std::rc::Rc;

thread_local! {
    static DEFAULT_STYLE: Rc<LayoutStyle> = Rc::new(LayoutStyle::default());
}

/// Layout state and caller-owned measurement data for one retained node.
///
/// Default styles share one thread-local object. The context can
/// contain a text resource ID, an intrinsic image size, or the caller's own data.
pub struct LayoutNode<C> {
    pub(crate) style: Rc<LayoutStyle>,
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
    pub fn with_style(style: LayoutStyle, context: C) -> Self {
        let mut node = Self::new(context);
        node.set_style(style);
        node
    }

    /// The node's layout style.
    pub fn style(&self) -> &LayoutStyle {
        &self.style
    }

    /// The layout engine's style.
    pub(crate) fn taffy_style(&self) -> &taffy::Style {
        &self.style.0
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

    /// How far the content, including padding, extends beyond the node's
    /// box on each axis as of the last layout: the scroll range of a viewport.
    pub fn scroll_size(&self) -> Size {
        Size::new(self.layout.scroll_width(), self.layout.scroll_height())
    }

    pub(crate) fn set_style(&mut self, style: LayoutStyle) {
        self.style = if style == LayoutStyle::default() {
            DEFAULT_STYLE.with(Rc::clone)
        } else {
            Rc::new(style)
        };
    }
}
