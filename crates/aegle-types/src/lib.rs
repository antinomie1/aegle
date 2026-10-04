//! Allocation-free geometry and compact colors shared by Aegle modules.
#![no_std]

mod color;
mod geometry;

pub use color::Color;
pub use geometry::{Point, Rect, Size};
