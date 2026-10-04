use std::num::NonZeroU64;

/// A tree-local slot and generation. Removing a node invalidates its ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NodeId(NonZeroU64);

impl NodeId {
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self(NonZeroU64::new(((generation as u64) << 32) | index as u64).unwrap())
    }

    /// Encodes this ID for adapters. This is not a persistent identity.
    pub const fn to_raw(self) -> u64 {
        self.0.get()
    }

    /// Decodes an adapter ID. Liveness is checked when accessing a tree.
    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw >> 32 == 0 {
            return None;
        }
        match NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    pub(crate) fn index(self) -> usize {
        self.to_raw() as u32 as usize
    }
    pub(crate) fn generation(self) -> u32 {
        (self.to_raw() >> 32) as u32
    }
}
