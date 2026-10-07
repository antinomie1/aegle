use crate::{NodeId, Tree};

/// Invalid event destination or scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RouteError {
    /// The root or destination is no longer alive.
    DeadNode,
    /// The destination is outside the supplied root's subtree.
    OutsideRoot,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DeadNode => "event root or destination is no longer alive",
            Self::OutsideRoot => "event destination is outside the root",
        })
    }
}
impl std::error::Error for RouteError {}

/// Delivery phase along a logical ancestor path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    /// Ancestors from root toward the destination, excluding the destination.
    Capture,
    /// The destination, delivered exactly once.
    Target,
    /// Ancestors from destination toward root, excluding the destination.
    Bubble,
}

/// One event delivery destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteStep {
    /// Logical node that receives this delivery.
    pub node: NodeId,
    /// Position in capture/target/bubble delivery.
    pub phase: EventPhase,
}

/// Reusable snapshot of the root-to-target logical path.
///
/// Storage grows only with ancestor depth. A route does not borrow the tree, so
/// callbacks may change it. Those changes do not change the current route:
/// callers must check that each destination is still alive before delivery.
/// IDs must come from the same tree; use one route per active event dispatch.
#[derive(Default, Debug)]
pub struct Route {
    path: Vec<NodeId>,
}

impl Route {
    /// Creates an empty route without allocating.
    pub const fn new() -> Self {
        Self { path: Vec::new() }
    }

    /// Replaces the route, validating both endpoints and their ancestry.
    /// An error clears the previous route while retaining its allocation.
    pub fn rebuild<T>(
        &mut self,
        tree: &Tree<T>,
        root: NodeId,
        target: NodeId,
    ) -> Result<(), RouteError> {
        self.path.clear();
        tree.get(root).ok_or(RouteError::DeadNode)?;
        let mut current = Some(target);
        while let Some(node) = current {
            current = match tree.parent(node) {
                Ok(parent) => parent,
                Err(_) => {
                    self.path.clear();
                    return Err(RouteError::DeadNode);
                }
            };
            self.path.push(node);
            if node == root {
                self.path.reverse();
                return Ok(());
            }
        }
        self.path.clear();
        Err(RouteError::OutsideRoot)
    }

    /// The captured path, including root and destination, or empty before use.
    pub fn path(&self) -> &[NodeId] {
        &self.path
    }

    /// Visits capture ancestors, the target, then bubble ancestors.
    /// This iterator borrows only the route, never the live tree.
    pub fn iter(&self) -> impl Iterator<Item = RouteStep> + '_ {
        let ancestors = &self.path[..self.path.len().saturating_sub(1)];
        ancestors
            .iter()
            .map(|&node| RouteStep {
                node,
                phase: EventPhase::Capture,
            })
            .chain(self.path.last().map(|&node| RouteStep {
                node,
                phase: EventPhase::Target,
            }))
            .chain(ancestors.iter().rev().map(|&node| RouteStep {
                node,
                phase: EventPhase::Bubble,
            }))
    }
}

/// Independent propagation and default-action decisions for one event.
///
/// The host checks these flags while delivering the route and before executing
/// the control's default behavior. Stopping propagation does not prevent that
/// behavior; preventing default behavior does not stop delivery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventControl {
    stopped: bool,
    prevented: bool,
}

impl EventControl {
    /// Stops delivery to subsequent destinations in this event's route.
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }

    /// Suppresses the host's default action for this event.
    pub fn prevent_default(&mut self) {
        self.prevented = true;
    }

    /// Whether the host should stop delivering this event.
    pub fn is_propagation_stopped(self) -> bool {
        self.stopped
    }

    /// Whether the host should suppress its default action.
    pub fn is_default_prevented(self) -> bool {
        self.prevented
    }
}

pub(crate) fn within<T>(tree: &Tree<T>, root: NodeId, mut node: NodeId) -> bool {
    loop {
        match tree.parent(node) {
            Ok(_) if node == root => return true,
            Ok(Some(parent)) => node = parent,
            _ => return false,
        }
    }
}
