//! Allocation-free geometry, compact colors and small input/system vocabulary shared by Aegle modules.
#![no_std]

#[cfg(feature = "color-math")]
extern crate std;

mod color;
#[cfg(feature = "color-math")]
pub mod color_math;
mod cursor;
mod geometry;
mod region;
mod shadow;
mod system;

pub use color::Color;
pub use cursor::Cursor;
pub use geometry::{PixelRect, Point, Rect, Size};
pub use region::{Area, Region};
pub use shadow::Shadow;
pub use system::{PointerButton, Preferences, TouchPhase};
