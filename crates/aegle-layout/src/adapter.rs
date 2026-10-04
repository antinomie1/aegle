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

pub(crate) struct Adapter<'a, C, F> {
    pub tree: &'a mut Tree<LayoutNode<C>>,
    pub measure: F,
}

impl<C, F> Adapter<'_, C, F> {
    fn node(&self, id: taffy::NodeId) -> &LayoutNode<C> {
        self.tree.get(from_taffy(id)).unwrap()
    }
    fn node_mut(&mut self, id: taffy::NodeId) -> &mut LayoutNode<C> {
        self.tree.get_mut(from_taffy(id)).unwrap()
    }
}

impl<C, F> TraversePartialTree for Adapter<'_, C, F> {
    type ChildIter<'a>
        = std::iter::Map<Children<'a, LayoutNode<C>>, fn(NodeId) -> taffy::NodeId>
    where
        Self: 'a;
    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        self.tree.children(from_taffy(id)).unwrap().map(to_taffy)
    }
    fn child_count(&self, id: taffy::NodeId) -> usize {
        self.tree.children(from_taffy(id)).unwrap().len()
    }
    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        to_taffy(self.tree.child(from_taffy(id), index).unwrap().unwrap())
    }
}

impl<C, F> LayoutPartialTree for Adapter<'_, C, F>
where
    F: FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
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
    }
    fn resolve_calc_value(&self, _: *const (), _: f32) -> f32 {
        unreachable!("calc values are unsupported")
    }

    fn compute_child_layout(&mut self, id: taffy::NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.compute_node(id, inputs, None)
    }
}

impl<C, F> Adapter<'_, C, F>
where
    F: FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
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
                return compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| unreachable!("calc values are unsupported"),
                    |known, available| {
                        (this.measure)(from_taffy(id), &mut node.context, known, available)
                    },
                );
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
    F: FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
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
    F: FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
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
    F: FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
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
