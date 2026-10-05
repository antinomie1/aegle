#![allow(unsafe_code)]

use ash::{Entry, vk};

use crate::{Error, Result};

/// The renderer owns this after every object allocated from its logical device.
/// Its queue is used serially; no surface or window connection is created here.
pub(crate) struct Device {
    pub raw: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub instance: ash::Instance,
    pub queue: vk::Queue,
    pub family: u32,
    pub properties: vk::PhysicalDeviceProperties,
    pub memory: vk::PhysicalDeviceMemoryProperties,
    // Vulkan function pointers remain valid only while their loader is loaded.
    _entry: Entry,
}

impl Device {
    pub fn new(device_index: Option<u32>) -> Result<Self> {
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
        let info = vk::InstanceCreateInfo::default().application_info(&app);
        // SAFETY: all pointed-to creation data outlives the call; no optional
        // extensions, layers or callbacks are requested.
        let instance = unsafe { entry.create_instance(&info, None) }.map_err(Error::Vulkan)?;
        let initialized = initialize(&instance, device_index);
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
                    _entry: entry,
                })
            }
            Err(error) => {
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

fn initialize(instance: &ash::Instance, index: Option<u32>) -> Result<Initialized> {
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
        let family = match compatible(instance, physical, &properties) {
            Ok(family) => family,
            Err(error) if index.is_some() => return Err(error),
            Err(_) => continue,
        };
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let info = vk::DeviceCreateInfo::default().queue_create_infos(&queues);
        // SAFETY: family supports graphics and has at least one queue. This
        // renderer requires no optional features or extensions at initialization.
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
    for (format, required, message) in [
        (
            vk::Format::R16G16B16A16_SFLOAT,
            vk::FormatFeatureFlags::COLOR_ATTACHMENT
                | vk::FormatFeatureFlags::COLOR_ATTACHMENT_BLEND
                | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            "RGBA16F color blending and sampling are required",
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
    unsafe { instance.get_physical_device_queue_family_properties(physical) }
        .iter()
        .position(|queue| {
            queue.queue_count > 0 && queue.queue_flags.contains(vk::QueueFlags::GRAPHICS)
        })
        .map(|index| index as u32)
        .ok_or(Error::Unsupported("a Vulkan graphics queue is required"))
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: the renderer owns queue submission exclusively and drops every
        // child Vulkan object before this owner. A lost device is still destroyed;
        // Drop cannot report its wait error. The loader remains alive throughout.
        unsafe {
            let _ = self.raw.device_wait_idle();
            self.raw.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
