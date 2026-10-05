//! Native rendering policy; retained scenes and clipping are shared by backends.
#![allow(unsafe_code)]

use crate::Result;
use crate::native::{RendererBackend, Runtime};
#[cfg(any(feature = "software", feature = "vulkan"))]
use {crate::Ui, aegle_scene::Affine, aegle_types::Rect};

pub(crate) fn validate_backend(renderer: RendererBackend) -> Result<()> {
    match renderer {
        RendererBackend::Software if !cfg!(feature = "software") => {
            Err("software rendering requires the software feature".into())
        }
        RendererBackend::Vulkan if !cfg!(feature = "vulkan") => {
            Err("Vulkan rendering requires the vulkan feature".into())
        }
        _ => Ok(()),
    }
}

#[cfg(feature = "vulkan")]
pub(crate) fn create_gpu(
    runtime: &Runtime,
    id: crate::platform::WindowId,
) -> Result<Option<aegle_render_vulkan::WindowRenderer<crate::platform::WindowSurface>>> {
    if runtime.options.renderer != RendererBackend::Vulkan {
        return Ok(None);
    }
    let window = runtime.backend.window_surface(id)?;
    // SAFETY: WindowSurface owns the native window and connection. Its public API
    // cannot revoke these handles; Runtime drops the presenter before its platform.
    Ok(Some(unsafe {
        aegle_render_vulkan::WindowRenderer::new(window, runtime.options.vulkan)?
    }))
}

#[cfg(any(feature = "software", feature = "vulkan"))]
fn scenes(
    ui: &Ui,
    factor: f32,
    mut draw: impl FnMut(&aegle_scene::Scene, Affine, Option<Rect>) -> Result<()>,
) -> Result<()> {
    let scale = Affine::scale(factor, factor)?;
    ui.visit_scenes(|scene, transform, clip| {
        let clip = clip.map(|rect| {
            Rect::new(
                rect.origin.x * factor,
                rect.origin.y * factor,
                rect.size.width * factor,
                rect.size.height * factor,
            )
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
            #[cfg(any(feature = "software", feature = "vulkan"))]
            let info = self.backend.window_info(entry.id)?;
            #[cfg(any(feature = "software", feature = "vulkan"))]
            let background = entry.ui.background();
            match self.options.renderer {
                #[cfg(feature = "software")]
                RendererBackend::Software => {
                    let renderer = self.renderer.as_mut().unwrap();
                    self.backend
                        .present(entry.id, |pixels, size| -> Result<()> {
                            let mut surface = aegle_render_software::Surface::new(
                                pixels,
                                size.width,
                                size.height,
                            )?;
                            let mut frame = renderer.begin_frame(&mut surface, background);
                            scenes(&entry.ui, info.scale as f32, |scene, transform, clip| {
                                frame.draw_clipped(scene, transform, clip)?;
                                Ok(())
                            })
                        })
                        .map_err(|error| format!("present: {error}"))?;
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
                    if !result.map_err(|error| format!("Vulkan present: {error}"))? {
                        self.backend.request_redraw(entry.id)?;
                    }
                }
                #[allow(unreachable_patterns)]
                _ => unreachable!("rendering feature was checked at App construction"),
            }
            // Native frame pacing blocks occluded windows without an idle poll.
            #[cfg(feature = "motion")]
            if entry.ui.has_animations() {
                self.backend.request_redraw(entry.id)?;
            }
        }
        Ok(())
    }
}
