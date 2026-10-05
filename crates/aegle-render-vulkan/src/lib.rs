//! Vulkan 1.1 rendering of retained scenes, independent of windows and UI trees.
//!
//! [`Renderer`] owns one reusable offscreen target and one submission at a time.
//! [`Frame::finish`] submits without readback; [`Renderer::wait`] or the next frame
//! waits on its fence. [`Renderer::read_pixels`] explicitly requests CPU readback.
//! Output matches Aegle's premultiplied sRGB RGBA8 byte convention; blending takes
//! place in a linear floating-point attachment before a final GPU encoding pass.
//!
//! Geometry supports at most eight simultaneous clips. Optional `text` adds
//! bounded, on-demand mask/color glyph atlases without a shaping dependency.
//! Without that feature, text commands return an error. Optional `window` adds
//! `WindowRenderer` for Wayland and Win32 surfaces with FIFO presentation.
//! Loading a Vulkan CPU driver is possible and is not proof
//! of hardware GPU acceleration; [`Renderer::device_name`] identifies the device.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "text")]
mod atlas;
mod commands;
mod device;
mod error;
mod geometry;
mod memory;
mod pipeline;
mod renderer;
#[cfg(feature = "window")]
mod surface;
#[cfg(feature = "window")]
mod swapchain;
mod target;
#[cfg(feature = "text")]
mod text;
#[cfg(feature = "text")]
mod text_pipeline;
#[cfg(feature = "text")]
mod upload;
#[cfg(feature = "window")]
mod window;

#[cfg(feature = "text")]
pub use atlas::{TextOptions, TextStats};
pub use error::{Error, Result};
pub use renderer::{Frame, Options, Renderer, Stats};
#[cfg(feature = "window")]
pub use window::WindowRenderer;
