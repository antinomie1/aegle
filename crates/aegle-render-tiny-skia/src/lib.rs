//! Software rendering with shared scenes, linear-light blending and bounded masks.
//!
//! This crate is the tiny-skia coverage backend of [`aegle_render_cpu`]: it
//! supplies antialiased geometry and arbitrary path/stroke coverage, with PNG
//! support disabled in normal builds. Compositing, clips, layers, blur, gradients,
//! shadows, images and glyph sampling are the shared CPU implementation, so this
//! backend's pixels differ from another CPU backend only where a rasterizer
//! flattens curves and samples edges.
//!
//! Framebuffers are borrowed. One coverage mask and one mask per active clip are
//! reused across frames; their total size is limited by the caller. tiny-skia's
//! transient path/scanline allocations are separate from the mask budget.

mod rasterizer;

pub use aegle_render_cpu::Renderer as CpuRenderer;
pub use aegle_render_cpu::{
    Area, FillRule, Frame, Mask, Rasterizer, RenderError, Segment, Surface,
};
pub use rasterizer::{TinyMask, TinyPath, TinySkia};

/// The CPU compositor over tiny-skia coverage.
pub type Renderer = CpuRenderer<TinySkia>;
