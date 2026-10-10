//! Compact retained trees independent of windows, renderers and layout.
//!
//! IDs are local to their tree. Child indices use four bytes and leaves allocate
//! no child storage. Structural edits invalidate ancestor layout automatically.

mod focus;
mod id;
mod state;
mod tree;

pub use focus::{Focus, FocusChange, FocusDirection, FocusError, FocusPolicy};
pub use id::NodeId;
pub use state::{Dirty, TreeError};
pub use tree::{Children, Tree};
