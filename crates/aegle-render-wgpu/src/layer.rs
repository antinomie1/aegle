//! Offscreen layers and backdrop blur: layer images are linear targets of
//! their own, composited through the image pipeline; a blur copies the
//! backdrop into scratch images and runs the shared box passes.

use aegle_gpu::{LayerPlan, Pixels, composite, empty, plan_blur, plan_layer, shifted};
use aegle_scene::{Affine, Layer, Rect, Scene};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, Extent3d, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDescriptor, StoreOp, Texture, TextureDescriptor,
    TextureDimension, TextureUsages, TextureView,
};

use crate::{Error, Renderer, Result, atlas::Slot, gpu::LINEAR};

/// A linear image with its view and the groups that sample it.
struct Image {
    texture: Texture,
    view: TextureView,
    /// Filtered sampling by the image pipeline.
    sampled: BindGroup,
    /// Texel loads by the blur pass.
    loaded: BindGroup,
    size: [u32; 2],
}

/// An open layer.
struct Open {
    /// `None` when nothing of the layer can show; its draws are dropped.
    image: Option<Image>,
    /// Where it lands in the target beneath.
    plan: LayerPlan,
    /// Its top left in the frame's coordinates.
    origin: [u32; 2],
    opacity: f32,
    /// A pass has drawn into it.
    loaded: bool,
}

/// Open layers and reusable images.
#[derive(Default)]
pub(crate) struct Layers {
    open: Vec<Open>,
    /// Released layer images, reused by later layers of the same size.
    spare: Vec<Image>,
    /// Backdrop copy and the two images the blur passes alternate between.
    scratch: Option<[Image; 3]>,
}

impl Renderer {
    fn layer_image(&self, size: [u32; 2]) -> Image {
        let texture = self.gpu.device.create_texture(&TextureDescriptor {
            label: Some("aegle layer"),
            size: Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: LINEAR,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC
                | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampled = self.gpu.device.create_bind_group(&BindGroupDescriptor {
            label: Some("aegle layer"),
            layout: &self.gpu.pages,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.gpu.sampler),
                },
            ],
        });
        let loaded = self.gpu.device.create_bind_group(&BindGroupDescriptor {
            label: Some("aegle blur source"),
            layout: self.gpu.resolve_layout(),
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&view),
            }],
        });
        Image {
            texture,
            view,
            sampled,
            loaded,
            size,
        }
    }

    /// The innermost layer's view and whether it was drawn into yet.
    pub(crate) fn layer_target(&self) -> Option<(&TextureView, bool)> {
        let open = self.layers.open.last()?;
        Some((&open.image.as_ref()?.view, open.loaded))
    }

    /// Records that a pass drew into the innermost layer; false without one.
    pub(crate) fn mark_layer_loaded(&mut self) -> bool {
        self.layers
            .open
            .last_mut()
            .map(|open| open.loaded = true)
            .is_some()
    }

    /// The frame-space origin and size of the current target.
    fn current(&self) -> ([u32; 2], [u32; 2]) {
        match self.layers.open.last() {
            Some(open) => (open.origin, open.image.as_ref().map_or([0; 2], |i| i.size)),
            None => ([0; 2], self.target.as_ref().unwrap().size),
        }
    }

    /// Walks a scene into the current target, shifted to its origin.
    pub(crate) fn walk_layered(
        &mut self,
        scene: &Scene,
        transform: Affine,
        clip: Option<Rect>,
    ) -> Result {
        let Some(open) = self.layers.open.last() else {
            return self.walk(scene, transform, clip);
        };
        if open.image.is_none() {
            return Ok(());
        }
        let [x, y] = open.origin;
        let shift = Affine::translation(-(x as f32), -(y as f32));
        let clip = clip.map(|rect| shifted(rect, open.origin));
        self.walk(scene, transform.then(shift)?, clip)
    }

    /// Opens a layer; see `Frame::push_layer`.
    pub(crate) fn push_layer(&mut self, layer: &Layer) -> Result {
        self.flush(None)?;
        let (origin, size) = self.current();
        if layer.backdrop_blur() > 0.0 && size != [0; 2] {
            self.blur_backdrop(layer, origin, size)?;
        }
        let plan = plan_layer(layer, origin, size);
        let extent = plan.extent;
        let image = (!empty(extent) && size != [0; 2]).then(|| {
            let wanted = [extent[2] - extent[0], extent[3] - extent[1]];
            match self.layers.spare.iter().position(|i| i.size == wanted) {
                Some(index) => self.layers.spare.swap_remove(index),
                None => self.layer_image(wanted),
            }
        });
        let open = Open {
            plan,
            origin: [origin[0] + extent[0], origin[1] + extent[1]],
            opacity: layer.opacity(),
            loaded: false,
            image,
        };
        self.layers.open.push(open);
        self.retarget();
        Ok(())
    }

    /// Composites the innermost layer onto the target beneath.
    pub(crate) fn pop_layer(&mut self) -> Result {
        if self.layers.open.is_empty() {
            return Err(Error::UnbalancedLayer);
        }
        self.flush(None)?;
        let open = self.layers.open.pop().unwrap();
        self.retarget();
        let Some(image) = open.image else {
            return Ok(());
        };
        if open.loaded {
            let at = [open.plan.extent[0], open.plan.extent[1]];
            let page = self.bind_frame_texture(&image.sampled);
            let (rec, viewport) = (&mut self.rec, self.viewport);
            composite(
                rec,
                image.size,
                at,
                open.plan.clip,
                None,
                open.opacity,
                viewport,
                page,
            )?;
            self.flush_full()?;
        }
        self.layers.spare.push(image);
        Ok(())
    }

    /// Pops every open layer, before the frame is finished.
    pub(crate) fn close_layers(&mut self) -> Result {
        while !self.layers.open.is_empty() {
            self.pop_layer()?;
        }
        Ok(())
    }

    /// Drops spare images at the start of a frame, keeping memory to what
    /// the previous frame's layers needed.
    pub(crate) fn begin_layers(&mut self) {
        self.layers.open.clear();
        let keep = self.layers.spare.len().min(8);
        self.layers.spare.truncate(keep);
    }

    /// Points the walker's target size and viewport at the current target.
    fn retarget(&mut self) {
        let (_, size) = self.current();
        self.size = size;
        self.viewport = aegle_gpu::viewport(size[0].max(1), size[1].max(1), true);
    }

    fn bind_frame_texture(&mut self, group: &BindGroup) -> u32 {
        let page = Slot::External(self.frame_textures.len() as u32).page();
        self.frame_textures.push(group.clone());
        page
    }

    /// Draws the blur of the current target under the layer shape over it.
    fn blur_backdrop(&mut self, layer: &Layer, origin: [u32; 2], size: [u32; 2]) -> Result {
        let Some(plan) = plan_blur(layer, origin, size) else {
            return Ok(());
        };
        let sampled: Pixels = plan.sampled;
        let extent = [sampled[2] - sampled[0], sampled[3] - sampled[1]];
        if self
            .layers
            .scratch
            .as_ref()
            .is_none_or(|images| images[0].size != extent)
        {
            self.layers.scratch = Some([0, 1, 2].map(|_| self.layer_image(extent)));
        }
        let source = match self.layers.open.last() {
            Some(open) => &open.image.as_ref().unwrap().texture,
            None => &self.target.as_ref().unwrap().image,
        };
        let images = self.layers.scratch.as_ref().unwrap();
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: sampled[0],
                    y: sampled[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            images[0].texture.as_image_copy(),
            Extent3d {
                width: extent[0],
                height: extent[1],
                depth_or_array_layers: 1,
            },
        );
        // The copy feeds the first pass; the others alternate between the
        // two remaining images, ending in the last.
        for (index, &pass) in plan.passes.iter().enumerate() {
            let from = if index == 0 { 0 } else { 2 - index % 2 };
            let to = 1 + index % 2;
            let mut render = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("aegle blur"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &images[to].view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            render.set_pipeline(&self.gpu.blur);
            render.set_bind_group(0, &images[from].loaded, &[]);
            render.draw(0..3, pass..pass + 1);
        }
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu.check()?;
        let group = images[2].sampled.clone();
        let page = self.bind_frame_texture(&group);
        let shift = Affine::translation(-(origin[0] as f32), -(origin[1] as f32));
        let shape = (layer.shape(), layer.transform().then(shift)?);
        let at = [sampled[0], sampled[1]];
        let (rec, viewport) = (&mut self.rec, self.viewport);
        composite(
            rec,
            extent,
            at,
            plan.area,
            Some(shape),
            layer.opacity(),
            viewport,
            page,
        )?;
        // The scratch images are reused by the next blur: draw this one now.
        self.flush(None)
    }
}
