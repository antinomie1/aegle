//! Vulkan 1.1 rendering of retained geometry, independent of windows and UI trees.
//!
//! [`Renderer`] owns one reusable offscreen target and one submission at a time.
//! [`Frame::finish`] submits without readback; [`Renderer::wait`] or the next frame
//! waits on its fence. [`Renderer::read_pixels`] explicitly requests CPU readback.
//! Output matches Aegle's premultiplied sRGB RGBA8 byte convention; blending takes
//! place in a linear floating-point attachment before a final GPU encoding pass.
//!
//! This backend currently supports geometry and at most eight simultaneous clips.
//! Text commands return an error. Native swapchains and UI-host selection are not
//! implemented here yet. Loading a Vulkan CPU driver is possible and is not proof
//! of hardware GPU acceleration; [`Renderer::device_name`] identifies the device.
#![deny(unsafe_op_in_unsafe_fn)]

mod commands;
mod device;
mod error;
mod geometry;
mod memory;
mod pipeline;
mod renderer;
mod target;

pub use error::{Error, Result};
pub use renderer::{Frame, Options, Renderer, Stats};
