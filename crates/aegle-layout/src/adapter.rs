use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{item::ItemStyle, node::LayoutNode};
use aegle_core::{Children, Dirty, NodeId, Tree};
use taffy::{
    AvailableSpace, CacheTree, Display, Layout, LayoutInput, LayoutOutput, LayoutPartialTree, Size,
    Style, TraversePartialTree, compute_cached_layout, compute_leaf_layout,
};

pub(crate) fn to_taffy(id: NodeId) -> taffy::NodeId {
    taffy::NodeId::new(id.to_raw())
}
fn from_taffy(id: taffy::NodeId) -> NodeId {
    NodeId::from_raw(id.into()).unwrap()
}

/// The host's leaf callbacks: content measurement and the first baseline.
pub(crate) trait Host<C> {
    fn measure(
        &mut self,
        id: NodeId,
        context: &mut C,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32>;
    fn baseline(&mut self, id: NodeId, context: &mut C, size: Size<f32>) -> Option<f32>;
}

impl<C, M, B> Host<C> for (M, B)
where
    M: FnMut(NodeId, &mut C, crate::Known, crate::Space) -> aegle_types::Size,
    B: FnMut(NodeId, &mut C, aegle_types::Size) -> Option<f32>,
{
    fn measure(
        &mut self,
        id: NodeId,
        context: &mut C,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        let axis = |space| match space {
            AvailableSpace::Definite(value) => crate::Available::Definite(value),
            AvailableSpace::MinContent => crate::Available::MinContent,
            AvailableSpace::MaxContent => crate::Available::MaxContent,
        };
        let known = crate::Known {
            width: known.width,
            height: known.height,
        };
        let space = crate::Space {
            width: axis(available.width),
            height: axis(available.height),
        };
        let size = (self.0)(id, context, known, space);
        Size {
            width: size.width,
            height: size.height,
        }
    }
    fn baseline(&mut self, id: NodeId, context: &mut C, size: Size<f32>) -> Option<f32> {
        (self.1)(id, context, aegle_types::Size::new(size.width, size.height))
    }
}

pub(crate) struct Adapter<'a, C, F> {
    pub tree: &'a mut Tree<LayoutNode<C>>,
    pub measure: F,
    /// Layout children of nodes with visible contents children, built once per
    /// computation; nodes without them use their tree children directly.
    pub flat: RefCell<HashMap<NodeId, Rc<[NodeId]>>>,
}

impl<C, F> Adapter<'_, C, F> {
    fn node(&self, id: taffy::NodeId) -> &LayoutNode<C> {
        self.tree.get(from_taffy(id)).unwrap()
    }
    fn node_mut(&mut self, id: taffy::NodeId) -> &mut LayoutNode<C> {
        self.tree.get_mut(from_taffy(id)).unwrap()
    }
    /// A contents node that passes its children through; hidden ones stay
    /// ordinary hidden children and hide their subtree.
    fn transparent(&self, id: NodeId) -> bool {
        let node = self.tree.get(id).unwrap();
        node.contents && node.style.0.display != Display::None
    }
    fn push_flat(&self, id: NodeId, out: &mut Vec<NodeId>) {
        for child in self.tree.children(id).unwrap() {
            if self.transparent(child) {
                self.push_flat(child, out);
            } else {
                out.push(child);
            }
        }
    }
    fn flat(&self, id: taffy::NodeId) -> Option<Rc<[NodeId]>> {
        let id = from_taffy(id);
        if let Some(children) = self.flat.borrow().get(&id) {
            return Some(children.clone());
        }
        if !self
            .tree
            .children(id)
            .unwrap()
            .any(|child| self.transparent(child))
        {
            return None;
        }
        let mut children = Vec::new();
        self.push_flat(id, &mut children);
        let children: Rc<[NodeId]> = children.into();
        self.flat.borrow_mut().insert(id, children.clone());
        Some(children)
    }
    /// Gives each visible contents descendant reached through contents nodes
    /// its layout ancestor's size at offset zero.
    fn place_contents(&mut self, id: NodeId, layout: &Layout) {
        let mut index = 0;
        while let Some(child) = self.tree.child(id, index).unwrap() {
            index += 1;
            if self.transparent(child) {
                let node = self.tree.get_mut(child).unwrap();
                node.layout = Layout {
                    location: taffy::Point::ZERO,
                    ..*layout
                };
                self.tree.clear_dirty(child, Dirty::LAYOUT).unwrap();
                self.place_contents(child, layout);
            }
        }
    }
}

/// Tree children, or the flattened children of a node with contents children.
pub(crate) enum ChildIter<'a, C> {
    Direct(Children<'a, LayoutNode<C>>),
    Flat(Rc<[NodeId]>, usize),
}

impl<C> Iterator for ChildIter<'_, C> {
    type Item = taffy::NodeId;
    fn next(&mut self) -> Option<taffy::NodeId> {
        match self {
            Self::Direct(children) => children.next().map(to_taffy),
            Self::Flat(children, index) => {
                let child = children.get(*index)?;
                *index += 1;
                Some(to_taffy(*child))
            }
        }
    }
}

impl<C, F> TraversePartialTree for Adapter<'_, C, F> {
    type ChildIter<'a>
        = ChildIter<'a, C>
    where
        Self: 'a;
    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        match self.flat(id) {
            Some(children) => ChildIter::Flat(children, 0),
            None => ChildIter::Direct(self.tree.children(from_taffy(id)).unwrap()),
        }
    }
    fn child_count(&self, id: taffy::NodeId) -> usize {
        match self.flat(id) {
            Some(children) => children.len(),
            None => self.tree.children(from_taffy(id)).unwrap().len(),
        }
    }
    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        match self.flat(id) {
            Some(children) => to_taffy(children[index]),
            None => to_taffy(self.tree.child(from_taffy(id), index).unwrap().unwrap()),
        }
    }
}

impl<C, F> LayoutPartialTree for Adapter<'_, C, F>
where
    F: Host<C>,
{
    type CustomIdent = String;
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    fn get_core_container_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).taffy_style()
    }
    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &Layout) {
        self.node_mut(id).layout = *layout;
        if self.flat.borrow().contains_key(&from_taffy(id)) {
            self.place_contents(from_taffy(id), layout);
        }
    }
    fn resolve_calc_value(&self, handle: *const (), basis: f32) -> f32 {
        crate::values::resolve_calc(handle, basis)
    }

    fn compute_child_layout(&mut self, id: taffy::NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.compute_node(id, inputs, None)
    }
}

impl<C, F> Adapter<'_, C, F>
where
    F: Host<C>,
{
    fn compute_node(
        &mut self,
        id: taffy::NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> LayoutOutput {
        if inputs.run_mode == taffy::RunMode::PerformHiddenLayout
            || self.node(id).taffy_style().display == Display::None
        {
            return taffy::compute_hidden_layout(self, id);
        }
        compute_cached_layout(self, id, inputs, |this, id, inputs| {
            let mut output = this.compute_uncached(id, inputs, block_ctx);
            scroll_minimum(this.node(id).taffy_style(), &inputs, &mut output);
            output
        })
    }

    fn compute_uncached(
        &mut self,
        id: taffy::NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> LayoutOutput {
        if self.child_count(id) == 0 {
            let node = self.tree.get_mut(from_taffy(id)).unwrap();
            let style = &node.style.0;
            let mut output = compute_leaf_layout(
                inputs,
                style,
                crate::values::resolve_calc,
                |known, available| {
                    self.measure
                        .measure(from_taffy(id), &mut node.context, known, available)
                },
            );
            // Parents read baselines only from a performed layout, whose
            // final size places the text.
            if inputs.run_mode == taffy::RunMode::PerformLayout {
                output.baselines.first =
                    self.measure
                        .baseline(from_taffy(id), &mut node.context, output.size);
            }
            return output;
        }
        match self.node(id).taffy_style().display {
            Display::Flex => taffy::compute_flexbox_layout(self, id, inputs),
            Display::Block => taffy::compute_block_layout(self, id, inputs, block_ctx),
            Display::FlowRoot => taffy::compute_block_layout(self, id, inputs, None),
            #[cfg(feature = "grid")]
            Display::Grid => taffy::compute_grid_layout(self, id, inputs),
            Display::None => unreachable!(),
        }
    }
}

/// A scrolling axis keeps its content to itself: measured under a min-content
/// constraint without a definite size, the node is only as large as its
/// padding, border and minimum size. Ancestors' automatic minimum sizes then
/// stop at the viewport, so a grown scroll view shrinks to the space it is
/// given and scrolls, instead of enlarging every ancestor up to the window.
fn scroll_minimum(style: &Style, inputs: &LayoutInput, output: &mut LayoutOutput) {
    use taffy::{MaybeResolve, ResolveOrZero};
    if inputs.run_mode != taffy::RunMode::ComputeSize {
        return;
    }
    let calc = crate::values::resolve_calc;
    let width = inputs.parent_size.width;
    let padding = style.padding.resolve_or_zero(width, calc);
    let border = style.border.resolve_or_zero(width, calc);
    let axes = [
        (
            style.overflow.x,
            inputs.known_dimensions.width,
            inputs.available_space.width,
            style.size.width.maybe_resolve(width, calc),
            style.min_size.width.maybe_resolve(width, calc),
            padding.horizontal_axis_sum() + border.horizontal_axis_sum(),
            &mut output.size.width,
        ),
        (
            style.overflow.y,
            inputs.known_dimensions.height,
            inputs.available_space.height,
            style
                .size
                .height
                .maybe_resolve(inputs.parent_size.height, calc),
            style
                .min_size
                .height
                .maybe_resolve(inputs.parent_size.height, calc),
            padding.vertical_axis_sum() + border.vertical_axis_sum(),
            &mut output.size.height,
        ),
    ];
    for (overflow, known, available, size, min, edges, out) in axes {
        if overflow == taffy::Overflow::Scroll
            && known.is_none()
            && available == AvailableSpace::MinContent
            && (size.is_none() || inputs.sizing_mode == taffy::SizingMode::ContentSize)
        {
            *out = min.unwrap_or(0.0).max(edges);
        }
    }
}

impl<C, F> CacheTree for Adapter<'_, C, F> {
    fn cache_get(&mut self, id: taffy::NodeId, inputs: &LayoutInput) -> Option<LayoutOutput> {
        let core_id = from_taffy(id);
        if self.tree.dirty(core_id).unwrap().intersects(Dirty::LAYOUT) {
            self.node_mut(id).cache.clear();
            self.tree.clear_dirty(core_id, Dirty::LAYOUT).unwrap();
        }
        self.node_mut(id).cache.get(inputs)
    }
    fn cache_store(&mut self, id: taffy::NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.node_mut(id).cache.store(input, output);
    }
    fn cache_clear(&mut self, id: taffy::NodeId) {
        self.node_mut(id).cache.clear();
        self.tree
            .clear_dirty(from_taffy(id), Dirty::LAYOUT)
            .unwrap();
    }
}

impl<C, F> taffy::LayoutFlexboxContainer for Adapter<'_, C, F>
where
    F: Host<C>,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = ItemStyle<'a>
    where
        Self: 'a;
    fn get_flexbox_container_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).taffy_style()
    }
    fn get_flexbox_child_style(&self, id: taffy::NodeId) -> ItemStyle<'_> {
        let mut parent = self.tree.parent(from_taffy(id)).unwrap();
        while let Some(contents) = parent.filter(|&p| self.transparent(p)) {
            parent = self.tree.parent(contents).unwrap();
        }
        let fixed = parent.is_some_and(|parent| {
            let style = self.tree.get(parent).unwrap().taffy_style();
            let overflow = match style.flex_direction {
                taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse => style.overflow.x,
                _ => style.overflow.y,
            };
            overflow == taffy::Overflow::Scroll
        });
        ItemStyle {
            style: self.node(id).taffy_style(),
            fixed,
        }
    }
}
impl<C, F> taffy::LayoutBlockContainer for Adapter<'_, C, F>
where
    F: Host<C>,
{
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;
    fn get_block_container_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).taffy_style()
    }
    fn get_block_child_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).taffy_style()
    }
    fn compute_block_child_layout(
        &mut self,
        id: taffy::NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> LayoutOutput {
        self.compute_node(id, inputs, block_ctx)
    }
}
#[cfg(feature = "grid")]
impl<C, F> taffy::LayoutGridContainer for Adapter<'_, C, F>
where
    F: Host<C>,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;
    fn get_grid_container_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).taffy_style()
    }
    fn get_grid_child_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).taffy_style()
    }
}
