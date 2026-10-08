//! Application images drawn by [`aegle_scene::Command::Texture`].
#![allow(unsafe_code)]

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};

use aegle_gpu::{Recording, State, Textured};
use aegle_scene::{Rect, TextureId};
use ash::vk;

use crate::{Error, Renderer, Result, text::Limits};

/// Descriptor sets reserved for application images drawn in one frame.
pub(crate) const FRAME_TEXTURES: u32 = 16;

/// Image views registered on one device, shared by every renderer using it.
#[derive(Default)]
pub(crate) struct Registry {
    views: RefCell<HashMap<u64, (vk::ImageView, [u32; 2])>>,
    next: Cell<u64>,
}

/// Raw Vulkan handles of a renderer's device, for rendering into images that
/// are then registered with [`Renderer::register_texture`]. Do not destroy them.
#[derive(Clone)]
pub struct RawDevice {
    /// The instance the device was created from.
    pub instance: ash::Instance,
    /// The selected physical device.
    pub physical: vk::PhysicalDevice,
    /// The logical device.
    pub device: ash::Device,
    /// The graphics queue Aegle submits to.
    pub queue: vk::Queue,
    /// That queue's family index.
    pub family: u32,
}

impl crate::device::Device {
    pub(crate) fn raw_handles(&self) -> RawDevice {
        RawDevice {
            instance: self.instance.clone(),
            physical: self.physical,
            device: self.raw.clone(),
            queue: self.queue,
            family: self.family,
        }
    }
}

impl Registry {
    /// See [`Renderer::register_texture`].
    ///
    /// # Safety
    /// As for [`Renderer::register_texture`].
    pub(crate) unsafe fn register(
        &self,
        view: vk::ImageView,
        extent: [u32; 2],
    ) -> Result<TextureId> {
        if extent[0] == 0 || extent[1] == 0 || view == vk::ImageView::null() {
            return Err(Error::InvalidSize);
        }
        let id = self.next.get();
        self.next.set(id + 1);
        self.views.borrow_mut().insert(id, (view, extent));
        Ok(TextureId(id))
    }

    pub(crate) fn unregister(&self, id: TextureId) -> bool {
        self.views.borrow_mut().remove(&id.0).is_some()
    }

    fn get(&self, id: TextureId) -> Option<(vk::ImageView, [u32; 2])> {
        self.views.borrow().get(&id.0).copied()
    }
}

impl Renderer {
    /// Registers an application image view for [`aegle_scene::SceneBuilder::texture`]
    /// on every renderer sharing this device. Level 0 is sampled bilinearly,
    /// clamped to its edge texels.
    ///
    /// # Safety
    /// `view` must be a 2D color view of a single-sample image created on
    /// [`Self::raw_device`] with `SAMPLED` usage and a filterable float format
    /// whose samples are linear premultiplied RGBA (for example
    /// `R8G8B8A8_SRGB` with premultiplied or opaque content), `extent` its
    /// level-0 size. Whenever a frame drawing it executes, the image must be in
    /// `SHADER_READ_ONLY_OPTIMAL` with prior writes made visible to fragment
    /// shader reads (a barrier at the end of the application's commands
    /// submitted earlier on the same queue suffices). The view must stay valid
    /// until it is unregistered and every frame that drew it has completed
    /// ([`Self::wait`]).
    pub unsafe fn register_texture(
        &self,
        view: vk::ImageView,
        extent: [u32; 2],
    ) -> Result<TextureId> {
        // SAFETY: forwarded to the caller.
        unsafe { self.device.textures.register(view, extent) }
    }

    /// Forgets a registered texture. Later frames drawing it fail with
    /// [`Error::UnknownTexture`]; it returns false for an unknown id.
    pub fn unregister_texture(&self, id: TextureId) -> bool {
        self.device.textures.unregister(id)
    }

    /// Raw handles of this renderer's device and queue.
    pub fn raw_device(&self) -> RawDevice {
        self.device.raw_handles()
    }
}

impl crate::text::Text {
    /// Records a registered image stretched over `rect`, binding it to one of
    /// this frame's reserved descriptor sets on first use.
    pub(crate) fn texture(
        &mut self,
        registry: &Registry,
        recording: &mut Recording,
        id: TextureId,
        rect: Rect,
        state: State,
        limits: Limits,
    ) -> Result {
        let (view, extent) = registry.get(id).ok_or(Error::UnknownTexture)?;
        let Some((area, inverse)) = aegle_gpu::stretch(extent, rect, state)? else {
            return Ok(());
        };
        let slot = match self.textures.iter().position(|&used| used == Some(id)) {
            Some(slot) => slot,
            None => {
                let slot = self
                    .textures
                    .iter()
                    .position(Option::is_none)
                    .ok_or(Error::TooManyTextures)?;
                self.textures[slot] = Some(id);
                // The set is free: the frame waited for the previous submission
                // and this frame has not used the set yet.
                self.pipeline.update(self.external_set(slot), view);
                slot
            }
        };
        recording.record(
            Textured {
                area,
                inverse,
                rect: [0.0, 0.0, extent[0] as f32, extent[1] as f32],
                contrast: 0.0,
                viewport: limits.viewport,
                color: [1.0; 4],
                kind: 3,
                page: self.external_set(slot),
            }
            .primitive(state.clip),
            state.bounds,
        )?;
        Ok(())
    }

    /// Binds an internal image (a layer or a blurred backdrop) to a free
    /// reserved set, or `None` when every set is taken this submission.
    pub(crate) fn bind_internal(&mut self, view: vk::ImageView) -> Option<u32> {
        let slot = self.textures.iter().position(Option::is_none)?;
        // Registered ids count up from zero and never reach this marker.
        self.textures[slot] = Some(TextureId(u64::MAX));
        self.pipeline.update(self.external_set(slot), view);
        Some(self.external_set(slot))
    }

    /// Releases this frame's texture bindings; called after the frame's fence.
    pub(crate) fn begin_textures(&mut self) {
        self.textures.fill(None);
    }
}
