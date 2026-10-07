use crate::{NodeId, RouteError, Tree, route::within};

/// A node's participation in a scoped focus traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPolicy {
    /// This node can receive focus, and its children are visited.
    Focusable,
    /// Skip this node but continue into its children, as for a plain container.
    Skip,
    /// Skip the whole subtree, as for hidden or disabled controls.
    Prune,
}

/// Direction through logical preorder, independent of drawing order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    /// Move to the next eligible node.
    Forward,
    /// Move to the preceding eligible node.
    Backward,
}

/// A focus transition containing only live nodes inside the requested scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FocusChange {
    /// Previous live destination, even when it has become disabled.
    pub previous: Option<NodeId>,
    /// New destination, or `None` when the scope has no eligible destination.
    pub current: Option<NodeId>,
}

impl FocusChange {
    /// Whether a live destination gained or lost focus.
    pub fn changed(self) -> bool {
        self.previous != self.current
    }
}

/// Invalid explicit focus request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FocusError {
    /// Invalid root, destination or ancestry.
    Route(RouteError),
    /// The destination is skipped or an ancestor prunes its subtree.
    NotFocusable,
}

impl From<RouteError> for FocusError {
    fn from(error: RouteError) -> Self {
        Self::Route(error)
    }
}
impl std::fmt::Display for FocusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Route(error) => error.fmt(f),
            Self::NotFocusable => f.write_str("destination is not eligible for focus"),
        }
    }
}
impl std::error::Error for FocusError {}

/// Scoped focus state over an existing tree, without a second topology.
///
/// One instance belongs to one tree and one host focus domain. The policy reads
/// node values; it must return consistent eligibility during each operation.
/// Traversal retains reusable scratch storage proportional to ancestor depth.
#[derive(Default, Debug)]
pub struct Focus {
    current: Option<NodeId>,
    ancestors: Vec<(NodeId, usize)>,
}

impl Focus {
    /// Creates empty focus state without allocating.
    pub const fn new() -> Self {
        Self {
            current: None,
            ancestors: Vec::new(),
        }
    }

    /// Returns the stored destination only while it remains alive.
    /// Hosts must reconcile eligibility after changing visibility or policy.
    pub fn current<T>(&self, tree: &Tree<T>) -> Option<NodeId> {
        self.current.filter(|&node| tree.get(node).is_some())
    }

    /// Sets or clears focus inside `root`, including the root itself.
    ///
    /// The policy must permit the destination and every ancestor up to `root`.
    /// Invalid requests leave focus unchanged. Removed/out-of-scope previous
    /// destinations are omitted from the returned transition.
    pub fn set<T>(
        &mut self,
        tree: &Tree<T>,
        root: NodeId,
        target: Option<NodeId>,
        mut policy: impl FnMut(NodeId, &T) -> FocusPolicy,
    ) -> Result<FocusChange, FocusError> {
        tree.get(root).ok_or(RouteError::DeadNode)?;
        if let Some(target) = target {
            tree.get(target).ok_or(RouteError::DeadNode)?;
            if !within(tree, root, target) {
                return Err(RouteError::OutsideRoot.into());
            }
            let mut node = target;
            loop {
                let eligibility = policy(node, tree.get(node).unwrap());
                if eligibility == FocusPolicy::Prune
                    || (node == target && eligibility != FocusPolicy::Focusable)
                {
                    return Err(FocusError::NotFocusable);
                }
                if node == root {
                    break;
                }
                node = tree.parent(node).unwrap().unwrap();
            }
        }
        Ok(self.change(tree, root, target))
    }

    /// Moves focus through eligible nodes in logical preorder.
    ///
    /// With no eligible current destination, starts at the first/last eligible
    /// node. `wrap` cycles at the boundary; otherwise focus stays at that edge.
    /// An empty eligible set clears focus. A removed root returns an error.
    /// The policy runs once for each visited node; pruned children are not read.
    pub fn advance<T>(
        &mut self,
        tree: &Tree<T>,
        root: NodeId,
        direction: FocusDirection,
        wrap: bool,
        mut policy: impl FnMut(NodeId, &T) -> FocusPolicy,
    ) -> Result<FocusChange, RouteError> {
        tree.get(root).ok_or(RouteError::DeadNode)?;
        self.ancestors.clear();
        let (mut first, mut last, mut before, mut after) = (None, None, None, None);
        let mut found = false;
        let mut node = Some(root);
        while let Some(current) = node {
            let eligibility = policy(current, tree.get(current).unwrap());
            if eligibility == FocusPolicy::Focusable {
                first.get_or_insert(current);
                if self.current == Some(current) {
                    found = true;
                    before = last;
                } else if found && after.is_none() {
                    after = Some(current);
                }
                last = Some(current);
            }
            node = if eligibility != FocusPolicy::Prune {
                tree.child(current, 0).unwrap()
            } else {
                None
            };
            if node.is_some() {
                self.ancestors.push((current, 1));
            } else {
                while let Some((parent, next)) = self.ancestors.last_mut() {
                    node = tree.child(*parent, *next).unwrap();
                    if node.is_some() {
                        *next += 1;
                        break;
                    }
                    self.ancestors.pop();
                }
            }
        }
        let target = match direction {
            FocusDirection::Forward if !found => first,
            FocusDirection::Backward if !found => last,
            FocusDirection::Forward => after.or(if wrap { first } else { self.current }),
            FocusDirection::Backward => before.or(if wrap { last } else { self.current }),
        };
        Ok(self.change(tree, root, target))
    }

    fn change<T>(&mut self, tree: &Tree<T>, root: NodeId, target: Option<NodeId>) -> FocusChange {
        let previous = self.current.filter(|&node| within(tree, root, node));
        self.current = target;
        FocusChange {
            previous,
            current: target,
        }
    }
}
