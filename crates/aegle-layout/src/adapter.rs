use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::node::LayoutNode;
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
    fn baseline(&mut self, id: NodeId, context: &C, size: Size<f32>) -> Option<f32>;
}

impl<C, M, B> Host<C> for (M, B)
where
    M: FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
    B: FnMut(NodeId, &C, Size<f32>) -> Option<f32>,
{
    fn measure(
        &mut self,
        id: NodeId,
        context: &mut C,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
    ) -> Size<f32> {
        (self.0)(id, context, known, available)
    }
    fn baseline(&mut self, id: NodeId, context: &C, size: Size<f32>) -> Option<f32> {
        (self.1)(id, context, size)
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
        node.contents && node.style.display != Display::None
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
        self.node(id).style()
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
            || self.node(id).style().display == Display::None
        {
            return taffy::compute_hidden_layout(self, id);
        }
        compute_cached_layout(self, id, inputs, |this, id, inputs| {
            if this.child_count(id) == 0 {
                let node = this.tree.get_mut(from_taffy(id)).unwrap();
                let style = &*node.style;
                let mut output = compute_leaf_layout(
                    inputs,
                    style,
                    crate::values::resolve_calc,
                    |known, available| {
                        this.measure
                            .measure(from_taffy(id), &mut node.context, known, available)
                    },
                );
                // Parents read baselines only from a performed layout, whose
                // final size places the text.
                if inputs.run_mode == taffy::RunMode::PerformLayout {
                    output.baselines.first =
                        this.measure
                            .baseline(from_taffy(id), &node.context, output.size);
                }
                return output;
            }
            match this.node(id).style().display {
                Display::Flex => taffy::compute_flexbox_layout(this, id, inputs),
                Display::Block => taffy::compute_block_layout(this, id, inputs, block_ctx),
                Display::FlowRoot => taffy::compute_block_layout(this, id, inputs, None),
                #[cfg(feature = "grid")]
                Display::Grid => taffy::compute_grid_layout(this, id, inputs),
                Display::None => unreachable!(),
            }
        })
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
        = &'a Style
    where
        Self: 'a;
    fn get_flexbox_container_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).style()
    }
    fn get_flexbox_child_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).style()
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
        self.node(id).style()
    }
    fn get_block_child_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).style()
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
        self.node(id).style()
    }
    fn get_grid_child_style(&self, id: taffy::NodeId) -> &Style {
        self.node(id).style()
    }
}
