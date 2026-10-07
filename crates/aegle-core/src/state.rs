use std::{
    fmt,
    ops::{BitOr, BitOrAssign},
};

/// Independent invalidation channels. Layout invalidation reaches ancestors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dirty(pub(crate) u8);

impl Dirty {
    /// No pending change.
    pub const NONE: Self = Self(0);
    /// Size, position or intrinsic measurement changed.
    pub const LAYOUT: Self = Self(1);
    /// Pixel appearance changed.
    pub const PAINT: Self = Self(2);
    /// Assistive-technology information changed.
    pub const SEMANTICS: Self = Self(4);
    /// All channels require an initial update.
    pub const ALL: Self = Self(7);

    /// Whether any channel in `other` is present.
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    /// Whether all channels are clear.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub(crate) fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl BitOr for Dirty {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}
impl BitOrAssign for Dirty {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// Invalid public tree operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TreeError {
    /// The node has been removed or its ID is invalid for this tree.
    DeadNode,
    /// Reparenting would introduce a cycle.
    Cycle,
    /// All representable slots have been used.
    Capacity,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DeadNode => "node is no longer alive",
            Self::Cycle => "reparenting would create a cycle",
            Self::Capacity => "tree slot capacity exhausted",
        })
    }
}
impl std::error::Error for TreeError {}
