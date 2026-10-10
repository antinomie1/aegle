//! Native rendering policy; retained scenes and clipping are shared by backends.
#![allow(unsafe_code)]

use crate::native::{RendererBackend, Runtime};
use aegle_ui::Result;
#[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
use {aegle_scene::Affine, aegle_types::Rect, aegle_ui::Ui};

pub(crate) fn validate_backend(renderer: RendererBackend) -> Result<()> {
    match renderer {
        RendererBackend::Software if !cfg!(feature = "software") => {
            Err("software rendering requires the software feature".into())
        }
        RendererBackend::Vulkan if !cfg!(feature = "vulkan") => {
            Err("Vulkan rendering requires the vulkan feature".into())
        }
        RendererBackend::Wgpu if !cfg!(feature = "wgpu") => {
            Err("wgpu rendering requires the wgpu feature".into())
        }
        _ => Ok(()),
    }
}

#[cfg(feature = "vulkan")]
pub(crate) fn create_gpu(
    runtime: &mut Runtime,
    id: crate::platform::WindowId,
) -> Result<Option<aegle_render_vulkan::WindowRenderer<crate::platform::WindowSurface>>> {
    if runtime.options.renderer != RendererBackend::Vulkan {
        return Ok(None);
    }
    let window = runtime.backend.window_surface(id)?;
    // SAFETY: WindowSurface owns the native window and connection. Its public API
    // cannot revoke these handles; Runtime drops the presenter before its platform.
    let renderer = unsafe {
        match &runtime.shared_vulkan {
            Some(shared) => aegle_render_vulkan::WindowRenderer::with_device(
                window,
                runtime.options.vulkan,
                shared,
            )?,
            None => aegle_render_vulkan::WindowRenderer::new(window, runtime.options.vulkan)?,
        }
    };
    runtime
        .shared_vulkan
        .get_or_insert_with(|| renderer.shared_device());
    Ok(Some(renderer))
}

#[cfg(feature = "wgpu")]
pub(crate) fn create_wgpu(
    runtime: &mut Runtime,
    id: crate::platform::WindowId,
) -> Result<Option<aegle_render_wgpu::WindowRenderer<crate::platform::WindowSurface>>> {
    if runtime.options.renderer != RendererBackend::Wgpu {
        return Ok(None);
    }
    let window = runtime.backend.window_surface(id)?;
    // SAFETY: WindowSurface owns the native window and connection. Its public API
    // cannot revoke these handles; Runtime drops the presenter before its platform.
    let renderer = unsafe {
        match &runtime.shared_wgpu {
            Some(shared) => {
                aegle_render_wgpu::WindowRenderer::with_gpu(window, runtime.options.wgpu, shared)?
            }
            None => aegle_render_wgpu::WindowRenderer::new(window, runtime.options.wgpu)?,
        }
    };
    runtime
        .shared_wgpu
        .get_or_insert_with(|| renderer.shared_gpu());
    Ok(Some(renderer))
}

/// A renderer frame the UI's records and layers are drawn into.
#[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
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

#[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
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
#[cfg(feature = "software")]
target!(aegle_render_software::Frame<'_, '_, '_>);
#[cfg(feature = "vulkan")]
target!(aegle_render_vulkan::Frame<'_>);
#[cfg(feature = "wgpu")]
target!(aegle_render_wgpu::Frame<'_>);

#[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
fn scenes(ui: &Ui, factor: f32, target: &mut impl Target) -> Result<()> {
    let scale = Affine::scale(factor, factor)?;
    // Node clips snap to whole device pixels, like a scissor rectangle; the
    // software renderer then clips without a surface-sized mask.
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
#[cfg(any(feature = "software", feature = "vulkan"))]
fn device_damage(
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

impl Runtime {
    // Without a renderer feature no App is created, and only the
    // unreachable arm remains.
    #[cfg_attr(
        not(any(feature = "software", feature = "vulkan", feature = "wgpu")),
        allow(unreachable_code)
    )]
    pub(crate) fn present(&mut self) -> Result<()> {
        for entry in &mut self.windows {
            if !std::mem::take(&mut entry.ready) {
                continue;
            }
            #[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
            let info = self.backend.window_info(entry.id)?;
            #[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
            let background = entry.ui.background();
            match self.options.renderer {
                #[cfg(feature = "software")]
                RendererBackend::Software => {
                    // Software windows are opaque on both platforms; GPU windows
                    // check their own surface, which may be transparent.
                    if background.to_rgba()[3] != 255 {
                        return Err("software windows require an opaque background".into());
                    }
                    let renderer = self.renderer.as_mut().unwrap();
                    let scale = info.scale;
                    // Only the changed area is redrawn into a retained buffer.
                    let size = info.buffer_size()?;
                    let damage = device_damage(&entry.ui, scale, size.width, size.height);
                    let presented = self
                        .backend
                        .present(entry.id, damage, |pixels, size, region| -> Result<()> {
                            let mut surface = aegle_render_software::Surface::new_bgra(
                                pixels,
                                size.width,
                                size.height,
                            )?;
                            for rect in region.rects() {
                                let rect = Rect::new(
                                    rect.x as f32,
                                    rect.y as f32,
                                    rect.width as f32,
                                    rect.height as f32,
                                );
                                let mut frame =
                                    renderer.begin_region(&mut surface, background, rect);
                                scenes(&entry.ui, scale, &mut frame)?;
                            }
                            Ok(())
                        })
                        .map_err(|error| format!("present: {error}"))?;
                    if presented {
                        entry.ui.clear_damage();
                    }
                }
                #[cfg(feature = "vulkan")]
                RendererBackend::Vulkan => {
                    let renderer = entry.gpu.as_mut().unwrap();
                    let result = self
                        .backend
                        .present_external(entry.id, |size| -> Result<bool> {
                            let mut frame =
                                match renderer.begin_frame(size.width, size.height, background) {
                                    Ok(Some(frame)) => frame,
                                    Ok(None)
                                    | Err(aegle_render_vulkan::Error::SurfaceOutOfDate) => {
                                        return Ok(false);
                                    }
                                    Err(error) => return Err(error.into()),
                                };
                            let [width, height] = frame.extent();
                            let scale = info.scale;
                            if let Some(damage) = device_damage(&entry.ui, scale, width, height) {
                                frame.set_damage(damage.rects());
                            }
                            scenes(&entry.ui, scale, &mut frame)?;
                            match frame.finish() {
                                Ok(()) => Ok(true),
                                Err(aegle_render_vulkan::Error::SurfaceOutOfDate) => Ok(false),
                                Err(error) => Err(error.into()),
                            }
                        });
                    if result.map_err(|error| format!("Vulkan present: {error}"))? {
                        entry.ui.clear_damage();
                    } else {
                        self.backend.request_redraw(entry.id)?;
                    }
                }
                #[cfg(feature = "wgpu")]
                RendererBackend::Wgpu => {
                    let renderer = entry.wgpu.as_mut().unwrap();
                    let result = self
                        .backend
                        .present_external(entry.id, |size| -> Result<bool> {
                            let mut frame =
                                match renderer.begin_frame(size.width, size.height, background) {
                                    Ok(Some(frame)) => frame,
                                    Ok(None) | Err(aegle_render_wgpu::Error::SurfaceOutOfDate) => {
                                        return Ok(false);
                                    }
                                    Err(error) => return Err(error.into()),
                                };
                            scenes(&entry.ui, info.scale, &mut frame)?;
                            frame.finish()?;
                            Ok(true)
                        });
                    if result.map_err(|error| format!("wgpu present: {error}"))? {
                        entry.ui.clear_damage();
                    } else {
                        self.backend.request_redraw(entry.id)?;
                    }
                }
                #[allow(unreachable_patterns)]
                _ => unreachable!("rendering feature was checked at App construction"),
            }
            // Native frame pacing blocks occluded windows without an idle poll.
            #[cfg(feature = "motion")]
            let animating = entry.ui.has_animations();
            #[cfg(not(feature = "motion"))]
            let animating = false;
            if animating || entry.ui.wants_frames() {
                self.backend.request_redraw(entry.id)?;
            }
        }
        Ok(())
    }
}
