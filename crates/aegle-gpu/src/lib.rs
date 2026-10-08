//! GPU-neutral pieces shared by Aegle's Vulkan and wgpu renderers.
//!
//! A [`Walker`] turns a retained [`aegle_scene::Scene`] into 112-byte
//! [`Primitive`] and 64-byte [`Clip`] storage rows in a [`Recording`]. The rows
//! match [`GEOMETRY_WGSL`], which both backends compile (Vulkan to SPIR-V at
//! build time, wgpu at run time), and [`RESOLVE_WGSL`] encodes the linear working
//! image into premultiplied sRGB. Commands that need a texture atlas (glyphs and,
//! with `vector`, images and paths) are handed back to the backend.
//!
//! Nothing here touches a graphics API, so the geometry, clip and placement rules
//! are tested once and behave identically on every backend.
mod effects;
mod error;
mod layer;
mod records;
mod shelf;
#[cfg(feature = "vector")]
mod vector;
mod walk;

pub use error::{Error, Result};
pub use layer::{
    BLUR_WGSL, BlurPlan, LayerPlan, Pixels, composite, empty, pixels, plan_blur, plan_layer,
    shifted,
};
pub use records::{
    Clip, MAX_PRIMITIVES, NO_CLIP, Primitive, Recording, State, Textured, bounds, viewport, visible,
};
pub use shelf::Shelf;
#[cfg(feature = "vector")]
pub use vector::{
    ImagePlacement, PathRaster, ResourceKey, image_placement, path_raster, rasterize_path, stretch,
};
pub use walk::{Step, Walker};
/// Reusable path-mask rasterizer storage.
#[cfg(feature = "vector")]
pub use zeno::Scratch;

/// Rounded-rectangle, stroke, gradient, shadow and atlas shaders: entries `vs_main`, `fs_main`, `fs_text`.
pub const GEOMETRY_WGSL: &str = include_str!("shader.wgsl");
/// Full-screen pass encoding the linear image into premultiplied sRGB.
pub const RESOLVE_WGSL: &str = include_str!("resolve.wgsl");
