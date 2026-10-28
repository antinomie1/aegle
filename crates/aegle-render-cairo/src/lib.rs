//! Linux CPU rendering of Aegle scenes through the system cairo library.
//!
//! This crate is the cairo coverage backend of [`aegle_render_cpu`]. Cairo
//! rasterizes geometry, paths and strokes into 8-bit coverage; the shared CPU
//! compositor decodes colors once, blends in linear light and quantizes to
//! premultiplied sRGB RGBA8, exactly as the tiny-skia backend does. The two
//! therefore agree byte for byte wherever coverage agrees.
//!
//! Nothing links cairo at build time. `libcairo.so.2` is opened on first use, so
//! an application keeps working on a system without it — [`Renderer::new`] then
//! reports [`RenderError::BackendUnavailable`] — and a distribution update cannot
//! change the build. Only the image surface, path and mask entry points are bound;
//! there is no window system, font, PDF or SVG backend involved, and the renderer
//! draws into a caller-owned buffer.
//!
//! Pixel alignment is a property of the compositor, not of cairo: upright
//! sharp-cornered fills and borders are computed analytically and never reach
//! cairo, so their pixels are identical on both CPU backends. Rounded corners,
//! arbitrary paths and strokes go through cairo's scanline rasterizer, whose
//! curve flattening and edge sampling differ from tiny-skia's by at most a few
//! coverage levels.

mod rasterizer;
mod sys;

pub use aegle_render_cpu::Renderer as CpuRenderer;
pub use aegle_render_cpu::{Area, FillRule, Frame, Mask};
pub use aegle_render_cpu::{Rasterizer, RenderError, Segment, Surface};
pub use rasterizer::{CairoMask, CairoPath, CairoRaster, status_text};

/// The CPU compositor over cairo coverage.
pub type Renderer = CpuRenderer<CairoRaster>;

/// The loaded cairo version, or `None` when `libcairo.so.2` is not installed.
pub fn version() -> Option<&'static str> {
    CairoRaster::version()
}

/// Creates a renderer, reporting a missing cairo library up front.
///
/// [`Renderer::default`] defers the same check to the first frame, since
/// [`Rasterizer`] implementations construct without I/O; a host that must fail at
/// startup uses this instead.
pub fn renderer(mask_budget: usize) -> Result<Renderer, RenderError> {
    Ok(Renderer::with_rasterizer(CairoRaster::new()?, mask_budget))
}
