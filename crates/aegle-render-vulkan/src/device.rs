#![allow(unsafe_code)]

use ash::{Entry, vk};

use crate::{Error, Result};

/// The renderer owns this after every object allocated from its logical device.
/// Its queue is used serially; optional surface ownership ends before the instance.
pub(crate) struct Device {
    pub raw: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub instance: ash::Instance,
    pub queue: vk::Queue,
    pub family: u32,
    pub properties: vk::PhysicalDeviceProperties,
    pub memory: vk::PhysicalDeviceMemoryProperties,
    #[cfg(feature = "window")]
    pub surface: Option<crate::surface::Surface>,
    // Vulkan function pointers remain valid only while their loader is loaded.
    _entry: Entry,
}

impl Device {
    pub fn new(device_index: Option<u32>) -> Result<Self> {
        Self::create(
            device_index,
            #[cfg(feature = "window")]
            None,
        )
    }

    #[cfg(feature = "window")]
    pub fn for_window(
        device_index: Option<u32>,
        display: raw_window_handle::RawDisplayHandle,
        window: raw_window_handle::RawWindowHandle,
    ) -> Result<Self> {
        Self::create(device_index, Some((display, window)))
    }

    fn create(
        device_index: Option<u32>,
        #[cfg(feature = "window")] handles: Option<(
            raw_window_handle::RawDisplayHandle,
            raw_window_handle::RawWindowHandle,
        )>,
    ) -> Result<Self> {
        // SAFETY: the system Vulkan loader implements the symbols ash loads. The
        // Entry remains owned until all instance/device handles are destroyed.
        let entry = unsafe { Entry::load() }.map_err(Error::Loader)?;
        // SAFETY: this is a loader query without handles or writable user memory.
        let version = unsafe { entry.try_enumerate_instance_version() }
            .map_err(Error::Vulkan)?
            .unwrap_or(vk::API_VERSION_1_0);
        if version < vk::API_VERSION_1_1 {
            return Err(Error::Unsupported("Vulkan 1.1 loader is required"));
        }
        let app = vk::ApplicationInfo::default()
            .application_name(c"Aegle")
            .engine_name(c"Aegle")
            .api_version(vk::API_VERSION_1_1);
        #[cfg(feature = "window")]
        let extensions = handles
            .map(|(display, _)| crate::surface::extensions(display))
            .transpose()?;
        let info = vk::InstanceCreateInfo::default().application_info(&app);
        #[cfg(feature = "window")]
        let info = if let Some(ref extensions) = extensions {
            info.enabled_extension_names(extensions)
        } else {
            info
        };
        // SAFETY: all pointed-to creation data outlives the call; no optional
        // layers or callbacks are requested; native surface extensions are optional.
        let instance = unsafe { entry.create_instance(&info, None) }.map_err(Error::Vulkan)?;
        #[cfg(feature = "window")]
        let surface = match handles
            .map(|(display, window)| {
                crate::surface::Surface::new(&entry, &instance, display, window)
            })
            .transpose()
        {
            Ok(surface) => surface,
            Err(error) => {
                // SAFETY: Surface construction failed and owns no live device.
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(error);
            }
        };
        let initialized = initialize(
            &instance,
            device_index,
            #[cfg(feature = "window")]
            surface.as_ref(),
        );
        match initialized {
            Ok((raw, physical, family, properties, memory)) => {
                // SAFETY: initialize created one queue at index zero in family.
                let queue = unsafe { raw.get_device_queue(family, 0) };
                Ok(Self {
                    raw,
                    physical,
                    instance,
                    queue,
                    family,
                    properties,
                    memory,
                    #[cfg(feature = "window")]
                    surface,
                    _entry: entry,
                })
            }
            Err(error) => {
                #[cfg(feature = "window")]
                drop(surface);
                // SAFETY: initialize has not retained a logical device on error.
                unsafe { instance.destroy_instance(None) };
                Err(error)
            }
        }
    }
}

type Initialized = (
    ash::Device,
    vk::PhysicalDevice,
    u32,
    vk::PhysicalDeviceProperties,
    vk::PhysicalDeviceMemoryProperties,
);

fn initialize(
    instance: &ash::Instance,
    index: Option<u32>,
    #[cfg(feature = "window")] surface: Option<&crate::surface::Surface>,
) -> Result<Initialized> {
    // SAFETY: instance is live for every physical-device query in this function.
    let devices = unsafe { instance.enumerate_physical_devices() }.map_err(Error::Vulkan)?;
    let mut candidates = devices
        .into_iter()
        .enumerate()
        .map(|(index, physical)| {
            // SAFETY: the handle was returned by this live instance.
            let properties = unsafe { instance.get_physical_device_properties(physical) };
            (index, physical, properties)
        })
        .collect::<Vec<_>>();
    if let Some(index) = index {
        candidates.retain(|(candidate, _, _)| *candidate == index as usize);
        if candidates.is_empty() {
            return Err(Error::Unsupported("Vulkan device index is out of range"));
        }
    } else {
        candidates.sort_by_key(|(_, _, properties)| match properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 0,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
            vk::PhysicalDeviceType::CPU => 3,
            _ => 2,
        });
    }
    for (_, physical, properties) in candidates {
        let family = match compatible(
            instance,
            physical,
            &properties,
            #[cfg(feature = "window")]
            surface,
        ) {
            Ok(family) => family,
            Err(error) if index.is_some() => return Err(error),
            Err(Error::Unsupported(_)) => continue,
            Err(error) => return Err(error),
        };
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let info = vk::DeviceCreateInfo::default().queue_create_infos(&queues);
        #[cfg(feature = "window")]
        let extensions = [ash::khr::swapchain::NAME.as_ptr()];
        #[cfg(feature = "window")]
        let info = if surface.is_some() {
            info.enabled_extension_names(&extensions)
        } else {
            info
        };
        // SAFETY: family supports graphics and has at least one queue. This
        // renderer requires no optional features; window mode enables swapchain only.
        let raw =
            unsafe { instance.create_device(physical, &info, None) }.map_err(Error::Vulkan)?;
        // SAFETY: physical belongs to instance, which still outlives this device.
        let memory = unsafe { instance.get_physical_device_memory_properties(physical) };
        return Ok((raw, physical, family, properties, memory));
    }
    Err(Error::Unsupported("no compatible Vulkan graphics device"))
}

fn compatible(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    properties: &vk::PhysicalDeviceProperties,
    #[cfg(feature = "window")] surface: Option<&crate::surface::Surface>,
) -> Result<u32> {
    if properties.api_version < vk::API_VERSION_1_1 {
        return Err(Error::Unsupported("Vulkan device does not support API 1.1"));
    }
    let limits = &properties.limits;
    if limits.max_push_constants_size < 112 {
        return Err(Error::Unsupported("112-byte push constants are required"));
    }
    if limits.max_storage_buffer_range < 64
        || limits.max_per_stage_descriptor_storage_buffers < 1
        || limits.max_descriptor_set_storage_buffers < 1
        || limits.max_bound_descriptor_sets < 1
    {
        return Err(Error::Unsupported("fragment storage buffers are required"));
    }
    if limits.max_per_stage_descriptor_sampled_images < 1
        || limits.max_descriptor_set_sampled_images < 1
        || limits.max_image_dimension2_d == 0
        || limits.max_framebuffer_width == 0
        || limits.max_framebuffer_height == 0
    {
        return Err(Error::Unsupported("sampled color attachments are required"));
    }
    #[cfg(feature = "text")]
    if limits.max_bound_descriptor_sets < 2
        || limits.max_per_stage_descriptor_samplers < 1
        || limits.max_descriptor_set_samplers < 1
    {
        return Err(Error::Unsupported("glyph texture sampling is required"));
    }
    for (format, required, message) in [
        (
            vk::Format::R16G16B16A16_SFLOAT,
            vk::FormatFeatureFlags::COLOR_ATTACHMENT
                | vk::FormatFeatureFlags::COLOR_ATTACHMENT_BLEND
                | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            "RGBA16F color blending and sampling are required",
        ),
        #[cfg(feature = "text")]
        (
            vk::Format::R8_UNORM,
            vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                | vk::FormatFeatureFlags::TRANSFER_DST,
            "R8 filtered glyph textures are required",
        ),
        #[cfg(feature = "text")]
        (
            vk::Format::R8G8B8A8_SRGB,
            vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                | vk::FormatFeatureFlags::TRANSFER_DST,
            "sRGB filtered glyph textures are required",
        ),
        (
            vk::Format::R8G8B8A8_UNORM,
            vk::FormatFeatureFlags::COLOR_ATTACHMENT | vk::FormatFeatureFlags::TRANSFER_SRC,
            "RGBA8 color attachment and transfer source are required",
        ),
    ] {
        // SAFETY: physical was enumerated from this live instance; format is a
        // core color format and no output pointers are supplied by the caller.
        let support = unsafe { instance.get_physical_device_format_properties(physical, format) };
        if !support.optimal_tiling_features.contains(required) {
            return Err(Error::Unsupported(message));
        }
    }
    // SAFETY: physical belongs to this live instance.
    for (index, queue) in unsafe { instance.get_physical_device_queue_family_properties(physical) }
        .iter()
        .enumerate()
    {
        if queue.queue_count == 0 || !queue.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            continue;
        }
        #[cfg(feature = "window")]
        if let Some(surface) = surface {
            match surface.supports(instance, physical, index as u32) {
                Ok(()) => {}
                Err(Error::Unsupported(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        return Ok(index as u32);
    }
    Err(Error::Unsupported(
        "a compatible Vulkan graphics/present queue is required",
    ))
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: the renderer owns queue submission exclusively and drops every
        // child Vulkan object before this owner. A lost device is still destroyed;
        // Drop cannot report its wait error. The loader remains alive throughout.
        unsafe {
            let _ = self.raw.device_wait_idle();
            self.raw.destroy_device(None);
            #[cfg(feature = "window")]
            drop(self.surface.take());
            self.instance.destroy_instance(None);
        }
    }
}
