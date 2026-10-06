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

#[cfg(any(feature = "software", feature = "vulkan", feature = "wgpu"))]
fn scenes(
    ui: &Ui,
    factor: f32,
    mut draw: impl FnMut(&aegle_scene::Scene, Affine, Option<Rect>) -> Result<()>,
) -> Result<()> {
    let scale = Affine::scale(factor, factor)?;
    ui.visit_scenes(|scene, transform, clip| {
        // Node clips snap to whole device pixels, like a scissor rectangle; the
        // software renderer then clips without a surface-sized mask.
        let clip = clip.map(|rect| {
            let (left, top) = (
                (rect.origin.x * factor).round(),
                (rect.origin.y * factor).round(),
            );
            let right = ((rect.origin.x + rect.size.width) * factor).round();
            let bottom = ((rect.origin.y + rect.size.height) * factor).round();
            Rect::new(left, top, right - left, bottom - top)
        });
        draw(scene, transform.then(scale)?, clip)
    })
}

impl Runtime {
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
                    let renderer = self.renderer.as_mut().unwrap();
                    let scale = info.scale as f32;
                    // Only the changed area is redrawn into a retained buffer.
                    let size = info.buffer_size()?;
                    let damage = entry.ui.damage()?.map(|rect| {
                        aegle_types::PixelRect::covering(rect, scale, size.width, size.height)
                    });
                    let presented = self
                        .backend
                        .present(entry.id, damage, |pixels, size, region| -> Result<()> {
                            let mut surface = aegle_render_software::Surface::new(
                                pixels,
                                size.width,
                                size.height,
                            )?;
                            let region = Rect::new(
                                region.x as f32,
                                region.y as f32,
                                region.width as f32,
                                region.height as f32,
                            );
                            let mut frame = renderer.begin_region(&mut surface, background, region);
                            scenes(&entry.ui, scale, |scene, transform, clip| {
                                frame.draw_clipped(scene, transform, clip)?;
                                Ok(())
                            })
                        })
                        .map_err(|error| format!("present: {error}"))?;
                    if presented {
                        entry.ui.clear_damage()?;
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
                            scenes(&entry.ui, info.scale as f32, |scene, transform, clip| {
                                frame.draw_clipped(scene, transform, clip)?;
                                Ok(())
                            })?;
                            match frame.finish() {
                                Ok(()) => Ok(true),
                                Err(aegle_render_vulkan::Error::SurfaceOutOfDate) => Ok(false),
                                Err(error) => Err(error.into()),
                            }
                        });
                    if result.map_err(|error| format!("Vulkan present: {error}"))? {
                        entry.ui.clear_damage()?;
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
                            scenes(&entry.ui, info.scale as f32, |scene, transform, clip| {
                                frame.draw_clipped(scene, transform, clip)?;
                                Ok(())
                            })?;
                            frame.finish()?;
                            Ok(true)
                        });
                    if result.map_err(|error| format!("wgpu present: {error}"))? {
                        entry.ui.clear_damage()?;
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
