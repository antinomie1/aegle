use crate::{Dirty, NodeId, TreeError};

const NONE: u32 = u32::MAX;

struct Node<T> {
    parent: u32,
    children: Vec<u32>,
    dirty: Dirty,
    value: T,
}

enum Entry<T> {
    Occupied(Node<T>),
    Vacant(u32),
}

struct Slot<T> {
    generation: u32,
    entry: Entry<T>,
}

/// A retained forest with reusable generational slots and indexed children.
///
/// IDs must originate from this tree. Values are stored inline; leaves allocate
/// no children. There are no per-node reference counts or platform dependencies.
pub struct Tree<T> {
    slots: Vec<Slot<T>>,
    free: u32,
    len: usize,
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Tree<T> {
    /// Creates an empty tree without allocating.
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: NONE,
            len: 0,
        }
    }

    /// Reserves the expected slot count to avoid incremental reallocation.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            ..Self::new()
        }
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no live nodes remain.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Bytes reserved by slot and child arrays, excluding allocations in `T`.
    pub fn allocated_bytes(&self) -> usize {
        self.slots.capacity() * size_of::<Slot<T>>()
            + self
                .slots
                .iter()
                .map(|slot| match &slot.entry {
                    Entry::Occupied(n) => n.children.capacity() * size_of::<u32>(),
                    _ => 0,
                })
                .sum::<usize>()
    }

    /// Inserts a root (`None`) or appends to a parent's children.
    pub fn insert(&mut self, parent: Option<NodeId>, value: T) -> Result<NodeId, TreeError> {
        if let Some(parent) = parent {
            self.node(parent)?;
        }
        let node = Node {
            parent: parent.map_or(NONE, |id| id.index() as u32),
            children: Vec::new(),
            dirty: Dirty::ALL,
            value,
        };
        let index = if self.free != NONE {
            let index = self.free;
            let slot = &mut self.slots[index as usize];
            let Entry::Vacant(next) = slot.entry else {
                unreachable!()
            };
            self.free = next;
            slot.entry = Entry::Occupied(node);
            index
        } else {
            if self.slots.len() >= NONE as usize {
                return Err(TreeError::Capacity);
            }
            let index = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 1,
                entry: Entry::Occupied(node),
            });
            index
        };
        self.len += 1;
        if let Some(parent) = parent {
            self.node_mut(parent)?.children.push(index);
            self.mark_dirty(parent, Dirty::ALL)?;
        }
        Ok(self.id_at(index))
    }

    /// Inserts a child before position `index`; positions past the end append.
    pub fn insert_at(
        &mut self,
        parent: NodeId,
        index: usize,
        value: T,
    ) -> Result<NodeId, TreeError> {
        let id = self.insert(Some(parent), value)?;
        let children = &mut self.node_mut(parent)?.children;
        let index = index.min(children.len() - 1);
        children[index..].rotate_right(1);
        Ok(id)
    }

    /// Accesses a live node's value.
    pub fn get(&self, id: NodeId) -> Option<&T> {
        self.node(id).ok().map(|n| &n.value)
    }

    /// Accesses stored data without invalidation, for caches and adapter state.
    /// Use [`Self::update`] for changes affecting layout, pixels or semantics.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.node_mut(id).ok().map(|n| &mut n.value)
    }

    /// Changes a value and records the affected channels.
    pub fn update<R>(
        &mut self,
        id: NodeId,
        dirty: Dirty,
        change: impl FnOnce(&mut T) -> R,
    ) -> Result<R, TreeError> {
        let result = change(&mut self.node_mut(id)?.value);
        self.mark_dirty(id, dirty)?;
        Ok(result)
    }

    /// Returns the logical parent, or `None` for a root.
    pub fn parent(&self, id: NodeId) -> Result<Option<NodeId>, TreeError> {
        let index = self.node(id)?.parent;
        Ok((index != NONE).then(|| self.id_at(index)))
    }

    /// Visits children in display order without allocating.
    pub fn children(&self, id: NodeId) -> Result<Children<'_, T>, TreeError> {
        Ok(Children {
            tree: self,
            indices: self.node(id)?.children.iter(),
        })
    }

    /// Returns a child in constant time, or `None` past the last child.
    pub fn child(&self, id: NodeId, index: usize) -> Result<Option<NodeId>, TreeError> {
        Ok(self
            .node(id)?
            .children
            .get(index)
            .map(|&index| self.id_at(index)))
    }

    /// Returns the pending invalidation channels.
    pub fn dirty(&self, id: NodeId) -> Result<Dirty, TreeError> {
        Ok(self.node(id)?.dirty)
    }

    /// Marks a node and propagates layout invalidation to every ancestor.
    pub fn mark_dirty(&mut self, id: NodeId, dirty: Dirty) -> Result<(), TreeError> {
        self.node_mut(id)?.dirty |= dirty;
        if dirty.intersects(Dirty::LAYOUT) {
            let mut parent = self.node(id)?.parent;
            while parent != NONE {
                let node = self.node_mut_at(parent);
                node.dirty |= Dirty::LAYOUT;
                parent = node.parent;
            }
        }
        Ok(())
    }

    /// Clears channels after the owning subsystem has consumed them.
    pub fn clear_dirty(&mut self, id: NodeId, dirty: Dirty) -> Result<(), TreeError> {
        self.node_mut(id)?.dirty.remove(dirty);
        Ok(())
    }

    /// Moves a subtree to the end of another parent's children, or makes a root.
    /// Rejected edits leave the original tree unchanged.
    pub fn reparent(&mut self, id: NodeId, parent: Option<NodeId>) -> Result<(), TreeError> {
        self.node(id)?;
        let mut ancestor = parent;
        while let Some(node) = ancestor {
            if node == id {
                return Err(TreeError::Cycle);
            }
            ancestor = self.parent(node)?;
        }
        self.detach(id)?;
        if let Some(parent) = parent {
            self.node_mut(parent)?.children.push(id.index() as u32);
            self.node_mut(id)?.parent = parent.index() as u32;
        }
        self.mark_dirty(id, Dirty::ALL)
    }

    /// Destroys a subtree in postorder, without recursion or temporary allocation.
    pub fn remove(&mut self, id: NodeId) -> Result<(), TreeError> {
        self.remove_with(id, |_, _| {})
    }

    /// Destroys a subtree and transfers each removed value to `removed`.
    /// Children are delivered before parents. The callback must not panic.
    pub fn remove_with(
        &mut self,
        id: NodeId,
        mut removed: impl FnMut(NodeId, T),
    ) -> Result<(), TreeError> {
        self.detach(id)?;
        let root = id.index() as u32;
        let mut current = root;
        loop {
            if let Some(child) = self.node_mut_at(current).children.pop() {
                current = child;
                continue;
            }
            let old_id = self.id_at(current);
            let slot = &mut self.slots[current as usize];
            let Entry::Occupied(node) = std::mem::replace(&mut slot.entry, Entry::Vacant(NONE))
            else {
                unreachable!()
            };
            if let Some(generation) = slot.generation.checked_add(1) {
                slot.generation = generation;
                slot.entry = Entry::Vacant(self.free);
                self.free = current;
            }
            self.len -= 1;
            removed(old_id, node.value);
            if current == root {
                break;
            }
            current = node.parent;
        }
        Ok(())
    }

    fn detach(&mut self, id: NodeId) -> Result<(), TreeError> {
        let parent = self.node(id)?.parent;
        if parent != NONE {
            let parent_id = self.id_at(parent);
            self.node_mut_at(parent)
                .children
                .retain(|&index| index != id.index() as u32);
            self.mark_dirty(parent_id, Dirty::ALL)?;
        }
        self.node_mut(id)?.parent = NONE;
        Ok(())
    }

    fn id_at(&self, index: u32) -> NodeId {
        NodeId::new(index, self.slots[index as usize].generation)
    }

    fn node(&self, id: NodeId) -> Result<&Node<T>, TreeError> {
        let slot = self.slots.get(id.index()).ok_or(TreeError::DeadNode)?;
        match &slot.entry {
            Entry::Occupied(node) if slot.generation == id.generation() => Ok(node),
            _ => Err(TreeError::DeadNode),
        }
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut Node<T>, TreeError> {
        let slot = self.slots.get_mut(id.index()).ok_or(TreeError::DeadNode)?;
        match &mut slot.entry {
            Entry::Occupied(node) if slot.generation == id.generation() => Ok(node),
            _ => Err(TreeError::DeadNode),
        }
    }

    fn node_mut_at(&mut self, index: u32) -> &mut Node<T> {
        match &mut self.slots[index as usize].entry {
            Entry::Occupied(node) => node,
            _ => unreachable!(),
        }
    }
}

/// An allocation-free, exact-size child iterator.
pub struct Children<'a, T> {
    tree: &'a Tree<T>,
    indices: std::slice::Iter<'a, u32>,
}
impl<T> Iterator for Children<'_, T> {
    type Item = NodeId;
    fn next(&mut self) -> Option<Self::Item> {
        self.indices.next().map(|&i| self.tree.id_at(i))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.indices.size_hint()
    }
}
impl<T> DoubleEndedIterator for Children<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.indices.next_back().map(|&i| self.tree.id_at(i))
    }
}
impl<T> ExactSizeIterator for Children<'_, T> {}
impl<T> std::iter::FusedIterator for Children<'_, T> {}
