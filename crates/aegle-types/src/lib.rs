//! Allocation-free geometry, compact colors and small input/system vocabulary shared by Aegle modules.
#![no_std]

#[cfg(any(feature = "color-math", feature = "std"))]
extern crate std;

mod color;
#[cfg(feature = "color-math")]
pub mod color_math;
mod cursor;
#[cfg(feature = "std")]
mod drag;
mod geometry;
mod region;
mod shadow;
mod system;

pub use color::Color;
pub use cursor::Cursor;
#[cfg(feature = "std")]
pub use drag::DragData;
#[cfg(all(feature = "std", unix))]
pub use drag::{file_uri, path_from_file_uri};
pub use geometry::{PixelRect, Point, Rect, Size};
pub use region::{Area, Region};
pub use shadow::Shadow;
pub use system::{PointerButton, Preferences, TouchPhase};
