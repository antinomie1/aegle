//! Application textures drawn by [`aegle_scene::Command::Texture`].
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

use aegle_scene::TextureId;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, Texture, TextureDimension,
    TextureSampleType, TextureUsages,
};

use crate::{Error, Renderer, Result, gpu::Gpu};

/// One registered texture: its bind group and texel size.
#[derive(Clone)]
pub(crate) struct Registered {
    pub group: BindGroup,
    pub size: [u32; 2],
}

/// Textures registered on one device, shared by every renderer using it.
#[derive(Default)]
pub(crate) struct Registry {
    textures: RefCell<HashMap<u64, Registered>>,
    next: Cell<u64>,
}

impl Gpu {
    /// Validates and registers an application texture for sampling.
    pub(crate) fn register_texture(&self, texture: &Texture) -> Result<TextureId> {
        let filterable = matches!(
            texture
                .format()
                .sample_type(None, Some(self.device.features())),
            Some(TextureSampleType::Float { filterable: true })
        );
        if texture.dimension() != TextureDimension::D2
            || texture.sample_count() != 1
            || !texture.usage().contains(TextureUsages::TEXTURE_BINDING)
            || !filterable
        {
            return Err(Error::Unsupported(
                "registered textures must be single-sample 2D, filterable float and bindable",
            ));
        }
        let view = texture.create_view(&Default::default());
        let group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("aegle external texture"),
            layout: &self.pages,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let registry = &self.external;
        let id = registry.next.get();
        registry.next.set(id + 1);
        registry.textures.borrow_mut().insert(
            id,
            Registered {
                group,
                size: [texture.width(), texture.height()],
            },
        );
        Ok(TextureId(id))
    }

    /// Forgets a texture; frames already recorded keep drawing it.
    pub(crate) fn unregister_texture(&self, id: TextureId) -> bool {
        self.external.textures.borrow_mut().remove(&id.0).is_some()
    }

    pub(crate) fn registered(&self, id: TextureId) -> Option<Registered> {
        self.external.textures.borrow().get(&id.0).cloned()
    }
}

impl Renderer {
    /// Registers an application texture for [`aegle_scene::SceneBuilder::texture`].
    /// It must be a single-sample 2D texture with `TEXTURE_BINDING` usage and a
    /// filterable float format whose samples are linear premultiplied RGBA (for
    /// example `Rgba8UnormSrgb` with premultiplied or opaque content), created
    /// on [`Self::device`]. Mip level 0 is sampled. Every renderer sharing this
    /// device can draw it. Render into it with commands submitted to
    /// [`Self::queue`] before the frame that draws it.
    pub fn register_texture(&self, texture: &Texture) -> Result<TextureId> {
        self.gpu.register_texture(texture)
    }

    /// Forgets a registered texture. Later frames that draw it fail with
    /// [`Error::UnknownTexture`]; it returns false for an unknown id.
    pub fn unregister_texture(&self, id: TextureId) -> bool {
        self.gpu.unregister_texture(id)
    }

    /// The device that textures for [`Self::register_texture`] must belong to.
    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    /// The queue this renderer submits to.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }

    /// Records a registered texture stretched over `rect`.
    pub(crate) fn texture(
        &mut self,
        id: TextureId,
        rect: aegle_scene::Rect,
        state: aegle_gpu::State,
    ) -> Result {
        let texture = self.gpu.registered(id).ok_or(Error::UnknownTexture)?;
        let Some((area, inverse)) = aegle_gpu::stretch(texture.size, rect, state)? else {
            return Ok(());
        };
        let page = crate::atlas::Slot::External(self.frame_textures.len() as u32).page();
        self.frame_textures.push(texture.group);
        let [width, height] = texture.size;
        self.rec.record(
            aegle_gpu::Textured {
                area,
                inverse,
                rect: [0.0, 0.0, width as f32, height as f32],
                contrast: 0.0,
                viewport: self.viewport,
                color: [1.0; 4],
                kind: 3,
                page,
            }
            .primitive(state.clip),
            state.bounds,
        )?;
        self.flush_full()
    }
}
