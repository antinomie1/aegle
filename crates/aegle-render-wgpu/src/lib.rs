//! Minimal portable GPU rendering of retained scenes through wgpu.
//!
//! [`Renderer`] draws a [`aegle_scene::Scene`] into one reusable offscreen target on
//! Vulkan, Metal or Direct3D 12, whichever the machine offers. Output matches
//! Aegle's premultiplied sRGB RGBA8 byte convention: blending happens in a linear
//! RGBA16F image and a final pass encodes it. Geometry supports at most eight
//! simultaneous clips. Optional `text` adds mask and color glyphs from one bounded
//! atlas per kind, images, and CPU-rasterized path masks; without it those
//! commands, like any future one, return [`Error::UnsupportedCommand`]. Optional `window`
//! adds [`WindowRenderer`] over any `raw-window-handle` surface with FIFO
//! presentation.
//!
//! The OpenGL backend is not enabled: it has no vertex-stage storage buffers.
//! A CPU Vulkan driver can be selected and is not proof of GPU acceleration;
//! [`Renderer::device_name`] identifies the adapter.
#[cfg(feature = "text")]
mod atlas;
mod error;
mod frame;
mod gpu;
mod renderer;
#[cfg(feature = "text")]
mod text;
#[cfg(feature = "text")]
mod vector;
#[cfg(feature = "window")]
mod window;

pub use error::{Error, Result};
pub use frame::Frame;
pub use renderer::{Options, Renderer};
#[cfg(feature = "window")]
pub use window::WindowRenderer;
