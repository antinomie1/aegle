//! Allocation-free geometry and compact colors shared by Aegle modules.
#![no_std]

#[cfg(feature = "color-math")]
extern crate std;

mod color;
#[cfg(feature = "color-math")]
pub mod color_math;
mod cursor;
mod geometry;

pub use color::Color;
pub use cursor::Cursor;
pub use geometry::{Point, Rect, Size};
