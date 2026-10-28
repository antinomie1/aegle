//! Shared drawing for every backend; retained scenes and clipping are common.
//!
//! Each backend is the same shape: ask the platform for the window's size, take
//! a frame from its own renderer, walk the retained tree into it, finish. The
//! backends differ in which frame type they hand back and whether the platform
//! presents its own software buffer or an external graphics API does. Those
//! differences live in the four `present_*` functions and their `create_*`
//! counterparts; everything above them is shared.
#![allow(unsafe_code)]

use crate::renderer::Present;
#[cfg(any(feature = "vulkan", feature = "wgpu"))]
use crate::renderer::Setup;
#[cfg(any(feature = "tiny-skia", feature = "cairo"))]
use aegle_platform::Platform;
#[cfg(any(feature = "vulkan", feature = "wgpu"))]
use aegle_platform::SurfacePlatform;
use aegle_platform::WindowId;
use aegle_ui::Result;

/// A lease on a window's native handles, for GPU presenters and adapters.
#[cfg(any(
    feature = "vulkan",
    feature = "wgpu",
    all(access, target_os = "windows"),
    all(access, target_os = "macos")
))]
pub(crate) type Surface = <crate::Native as aegle_platform::SurfacePlatform>::Surface;
#[cfg(any(
    feature = "tiny-skia",
    feature = "cairo",
    feature = "vulkan",
    feature = "wgpu"
))]
use {aegle_scene::Affine, aegle_types::Rect, aegle_ui::Ui};

/// A renderer frame the UI's records and layers are drawn into.
#[cfg(any(
    feature = "tiny-skia",
    feature = "cairo",
    feature = "vulkan",
    feature = "wgpu"
))]
trait Target {
    fn scene(
        &mut self,
        scene: &aegle_scene::Scene,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result<()>;
    fn push(&mut self, layer: &aegle_scene::Layer) -> Result<()>;
    fn pop(&mut self) -> Result<()>;
}

#[cfg(any(feature = "vulkan", feature = "wgpu"))]
macro_rules! target {
    ($frame:ty) => {
        impl Target for $frame {
            fn scene(
                &mut self,
                scene: &aegle_scene::Scene,
                transform: Affine,
                clip: Option<Rect>,
            ) -> Result<()> {
                Ok(self.draw_clipped(scene, transform, clip)?)
            }
            fn push(&mut self, layer: &aegle_scene::Layer) -> Result<()> {
                Ok(self.push_layer(layer)?)
            }
            fn pop(&mut self) -> Result<()> {
                Ok(self.pop_layer()?)
            }
        }
    };
}
/// The shared CPU frame over any coverage rasterizer.
///
/// `present_cpu` is generic so one function serves both CPU backends; the
/// frame type it draws into is generic too, so the `Target` bound must hold for
/// every rasterizer rather than only the two built here.
#[cfg(any(feature = "tiny-skia", feature = "cairo"))]
impl<R: aegle_render_cpu::Rasterizer> Target for aegle_render_cpu::Frame<'_, '_, '_, R> {
    fn scene(
        &mut self,
        scene: &aegle_scene::Scene,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result<()> {
        Ok(self.draw_clipped(scene, transform, clip)?)
    }

    fn push(&mut self, layer: &aegle_scene::Layer) -> Result<()> {
        Ok(self.push_layer(layer)?)
    }

    fn pop(&mut self) -> Result<()> {
        Ok(self.pop_layer()?)
    }
}
#[cfg(feature = "vulkan")]
target!(aegle_render_vulkan::Frame<'_>);
#[cfg(feature = "wgpu")]
target!(aegle_render_wgpu::Frame<'_>);

#[cfg(any(
    feature = "tiny-skia",
    feature = "cairo",
    feature = "vulkan",
    feature = "wgpu"
))]
fn scenes(ui: &Ui, factor: f32, target: &mut impl Target) -> Result<()> {
    let scale = Affine::scale(factor, factor);
    // Node clips snap to whole device pixels, like a scissor rectangle; the
    // CPU renderers then clip without a surface-sized mask.
    let snap = |rect: Rect| {
        let (left, top) = (
            (rect.origin.x * factor).round(),
            (rect.origin.y * factor).round(),
        );
        let right = ((rect.origin.x + rect.size.width) * factor).round();
        let bottom = ((rect.origin.y + rect.size.height) * factor).round();
        Rect::new(left, top, right - left, bottom - top)
    };
    ui.visit_scenes(|visit| match visit {
        aegle_ui::Visit::Scene {
            scene,
            transform,
            clip,
        } => target.scene(scene, transform.then(scale)?, clip.map(snap)),
        aegle_ui::Visit::PushLayer(layer) => {
            let clip = layer.clip().map(snap);
            target.push(&layer.then(scale)?.with_clip(clip)?)
        }
        aegle_ui::Visit::PopLayer => target.pop(),
    })
}

/// The UI's changed area in buffer pixels of a `width` × `height` window at
/// `scale`; `None` for all of it.
#[cfg(any(feature = "tiny-skia", feature = "cairo", feature = "vulkan"))]
pub(crate) fn device_damage(
    ui: &Ui,
    scale: f32,
    width: u32,
    height: u32,
) -> Option<aegle_types::Region<aegle_types::PixelRect>> {
    ui.damage().map(|logical| {
        let mut pixels = aegle_types::Region::default();
        for &rect in logical.rects() {
            pixels.add(aegle_types::PixelRect::covering(rect, scale, width, height));
        }
        pixels
    })
}

/// Draws one CPU renderer's frame into the platform's retained software buffer.
///
/// Both CPU backends present the same way: the platform lends its buffer, the
/// renderer composites the changed region into it, and only whole-pixel damage
/// is redrawn. A window background must be opaque because the XRGB/DIB surface
/// has no alpha to composite against.
#[cfg(any(feature = "tiny-skia", feature = "cairo"))]
pub(crate) fn present_cpu<R: aegle_render_cpu::Rasterizer>(
    renderer: &mut aegle_render_cpu::Renderer<R>,
    window: WindowId,
    present: &mut Present<'_>,
) -> Result<bool> {
    if present.background.to_rgba()[3] != 255 {
        return Err("a native software window requires an opaque background".into());
    }
    let Present {
        backend,
        ui,
        info,
        background,
    } = present;
    let scale = info.scale;
    // Only the changed area is redrawn into the retained buffer.
    let damage = device_damage(ui, scale, info.buffer.width, info.buffer.height);
    let presented = backend.present(window, damage, |pixels, size, region| {
        let mut surface = aegle_render_cpu::Surface::new_bgra(pixels, size.width, size.height)?;
        for rect in region.rects() {
            let rect = Rect::new(
                rect.x as f32,
                rect.y as f32,
                rect.width as f32,
                rect.height as f32,
            );
            let mut frame = renderer.begin_region(&mut surface, *background, rect);
            scenes(ui, scale, &mut frame)?;
        }
        Ok(())
    })?;
    Ok(presented)
}

/// Creates a Vulkan presenter for a new window, sharing `shared` when present.
#[cfg(feature = "vulkan")]
pub(crate) fn create_vulkan(
    setup: &mut Setup<'_>,
    window: WindowId,
    shared: &Option<aegle_render_vulkan::SharedDevice>,
) -> Result<aegle_render_vulkan::WindowRenderer<Surface>> {
    let lease = setup.backend.window_surface(window)?;
    // SAFETY: WindowSurface owns the native window and connection. Its public API
    // cannot revoke these handles; the renderer is dropped before its platform.
    unsafe {
        match shared {
            Some(shared) => {
                aegle_render_vulkan::WindowRenderer::with_device(lease, setup.vulkan(), shared)
            }
            None => aegle_render_vulkan::WindowRenderer::new(lease, setup.vulkan()),
        }
    }
    .map_err(Into::into)
}

/// Draws one Vulkan frame straight into the window's swapchain.
#[cfg(feature = "vulkan")]
pub(crate) fn present_vulkan(
    renderer: &mut aegle_render_vulkan::WindowRenderer<Surface>,
    window: WindowId,
    present: &mut Present<'_>,
) -> Result<bool> {
    let Present {
        backend,
        ui,
        info,
        background,
        ..
    } = present;
    let scale = info.scale;
    let presented = backend.present_external(window, |size| -> Result<bool> {
        let mut frame = match renderer.begin_frame(size.width, size.height, *background) {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(aegle_render_vulkan::Error::SurfaceOutOfDate) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let [width, height] = frame.extent();
        if let Some(damage) = device_damage(ui, scale, width, height) {
            frame.set_damage(damage.rects());
        }
        scenes(ui, scale, &mut frame)?;
        match frame.finish() {
            Ok(()) => Ok(true),
            Err(aegle_render_vulkan::Error::SurfaceOutOfDate) => Ok(false),
            Err(error) => Err(error.into()),
        }
    })?;
    Ok(presented)
}

/// Creates a wgpu presenter for a new window, sharing `shared` when present.
#[cfg(feature = "wgpu")]
pub(crate) fn create_wgpu(
    setup: &mut Setup<'_>,
    window: WindowId,
    shared: &Option<aegle_render_wgpu::SharedGpu>,
) -> Result<aegle_render_wgpu::WindowRenderer<Surface>> {
    let lease = setup.backend.window_surface(window)?;
    // SAFETY: WindowSurface owns the native window and connection. Its public API
    // cannot revoke these handles; the renderer is dropped before its platform.
    unsafe {
        match shared {
            Some(shared) => {
                aegle_render_wgpu::WindowRenderer::with_gpu(lease, setup.wgpu(), shared)
            }
            None => aegle_render_wgpu::WindowRenderer::new(lease, setup.wgpu()),
        }
    }
    .map_err(Into::into)
}

/// Draws one wgpu frame straight into the window's surface.
#[cfg(feature = "wgpu")]
pub(crate) fn present_wgpu(
    renderer: &mut aegle_render_wgpu::WindowRenderer<Surface>,
    window: WindowId,
    present: &mut Present<'_>,
) -> Result<bool> {
    let Present {
        backend,
        ui,
        info,
        background,
        ..
    } = present;
    let scale = info.scale;
    let presented = backend.present_external(window, |size| -> Result<bool> {
        let mut frame = match renderer.begin_frame(size.width, size.height, *background) {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(aegle_render_wgpu::Error::SurfaceOutOfDate) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        scenes(ui, scale, &mut frame)?;
        frame.finish()?;
        Ok(true)
    })?;
    Ok(presented)
}
