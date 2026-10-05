#![allow(unsafe_code)]

use ash::vk;

use crate::{Error, Result, device::Device};

/// Private renderer resources. The renderer fences GPU use before mapping or
/// dropping them, and drops the owning Device after all of these wrappers.
pub(crate) struct Buffer {
    pub handle: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub allocation: u64,
    pub size: u64,
    device: ash::Device,
    properties: vk::MemoryPropertyFlags,
}

impl Buffer {
    pub fn new(
        device: &Device,
        size: u64,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
        budget: u64,
    ) -> Result<Self> {
        if size == 0
            || (usage.contains(vk::BufferUsageFlags::STORAGE_BUFFER)
                && size > u64::from(device.properties.limits.max_storage_buffer_range))
        {
            return Err(Error::InvalidSize);
        }
        if usage.is_empty() {
            return Err(Error::InvalidState("buffer usage must be nonempty"));
        }
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY: device is live and the requested nonzero size satisfies the
        // storage-buffer binding limit when that usage is requested.
        let handle = unsafe { device.raw.create_buffer(&info, None) }.map_err(Error::Vulkan)?;
        let mut buffer = Self {
            handle,
            memory: vk::DeviceMemory::null(),
            allocation: 0,
            size,
            device: device.raw.clone(),
            properties: vk::MemoryPropertyFlags::empty(),
        };
        // SAFETY: handle was just created from device and is not yet bound.
        let requirements = unsafe { device.raw.get_buffer_memory_requirements(handle) };
        check_budget(requirements.size, budget)?;
        let index = memory_type(device, requirements.memory_type_bits, properties)?;
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().buffer(handle);
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(index)
            .push_next(&mut dedicated);
        // SAFETY: the selected memory type and exact allocation size satisfy the
        // buffer requirements. One allocation per resource is marked dedicated,
        // also satisfying drivers that require it. Errors release partial state.
        buffer.memory =
            unsafe { device.raw.allocate_memory(&info, None) }.map_err(Error::Vulkan)?;
        buffer.allocation = requirements.size;
        buffer.properties = device.memory.memory_types[index as usize].property_flags;
        // SAFETY: offset zero meets the buffer alignment; allocation matches its
        // requirements and is owned exclusively by this buffer.
        unsafe { device.raw.bind_buffer_memory(handle, buffer.memory, 0) }
            .map_err(Error::Vulkan)?;
        Ok(buffer)
    }

    /// Writes an in-bounds range after the renderer has fenced previous GPU use.
    /// The private wrapper never exposes a mapped slice to another caller.
    pub fn write(&mut self, offset: u64, bytes: &[u8]) -> Result {
        let offset = self.range(offset, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
        let coherent = self
            .properties
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);
        let mapped = self.map()?;
        // SAFETY: the range is within the buffer and allocation, mapping is
        // exclusive, the source cannot alias an unexposed mapping, and GPU use
        // is complete under the renderer's resource-lifetime contract.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.pointer.add(offset), bytes.len());
        }
        if !coherent {
            let ranges = [vk::MappedMemoryRange::default()
                .memory(mapped.memory)
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            // SAFETY: the entire allocation is mapped. Offset zero and WHOLE_SIZE
            // satisfy nonCoherentAtomSize alignment even at the allocation end.
            unsafe { mapped.device.flush_mapped_memory_ranges(&ranges) }.map_err(Error::Vulkan)?;
        }
        Ok(())
    }

    /// Reads an initialized range after the renderer's GPU completion fence.
    pub fn read(&mut self, offset: u64, bytes: &mut [u8]) -> Result {
        let offset = self.range(offset, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
        let coherent = self
            .properties
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);
        let mapped = self.map()?;
        if !coherent {
            let ranges = [vk::MappedMemoryRange::default()
                .memory(mapped.memory)
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            // SAFETY: the allocation is fully mapped and the renderer completed
            // device writes. Whole-allocation invalidation meets atom alignment.
            unsafe { mapped.device.invalidate_mapped_memory_ranges(&ranges) }
                .map_err(Error::Vulkan)?;
        }
        // SAFETY: the renderer initializes this range before reading it; the
        // checked range is mapped, exclusive, and cannot overlap the output.
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.pointer.add(offset),
                bytes.as_mut_ptr(),
                bytes.len(),
            );
        }
        Ok(())
    }

    fn range(&self, offset: u64, length: usize) -> Result<usize> {
        if !self
            .properties
            .contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
        {
            return Err(Error::Unsupported("buffer memory is not host visible"));
        }
        let end = offset
            .checked_add(length as u64)
            .ok_or(Error::InvalidSize)?;
        if end > self.size || end > isize::MAX as u64 {
            return Err(Error::InvalidSize);
        }
        Ok(offset as usize)
    }

    fn map(&mut self) -> Result<Mapping<'_>> {
        // SAFETY: only these exclusive methods map this allocation; HOST_VISIBLE
        // was checked by range. No GPU access overlaps under the renderer contract.
        let pointer = unsafe {
            self.device
                .map_memory(self.memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
        }
        .map_err(Error::Vulkan)?;
        Ok(Mapping {
            device: &self.device,
            memory: self.memory,
            pointer: pointer.cast(),
        })
    }
}

struct Mapping<'a> {
    device: &'a ash::Device,
    memory: vk::DeviceMemory,
    pointer: *mut u8,
}

impl Drop for Mapping<'_> {
    fn drop(&mut self) {
        // SAFETY: this guard owns the allocation's sole active mapping, including
        // when flushing or invalidating failed. The buffer cannot be dropped yet.
        unsafe { self.device.unmap_memory(self.memory) };
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: renderer fences commands before releasing resources and keeps
        // Device alive. Destroy the binding before its memory; null memory denotes
        // construction that failed before allocation and needs no release.
        unsafe {
            self.device.destroy_buffer(self.handle, None);
            if self.memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.memory, None);
            }
        }
    }
}

pub(crate) struct Image {
    pub handle: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub allocation: u64,
    device: ash::Device,
}

impl Image {
    pub fn new(
        device: &Device,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        budget: u64,
    ) -> Result<Self> {
        let limits = &device.properties.limits;
        if width == 0
            || height == 0
            || width > limits.max_image_dimension2_d
            || height > limits.max_image_dimension2_d
            || (usage.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                && (width > limits.max_framebuffer_width || height > limits.max_framebuffer_height))
        {
            return Err(Error::InvalidSize);
        }
        if !matches!(
            format,
            vk::Format::R8G8B8A8_UNORM | vk::Format::R16G16B16A16_SFLOAT
        ) {
            return Err(Error::Unsupported(
                "image wrapper supports renderer color formats only",
            ));
        }
        if usage.is_empty() {
            return Err(Error::InvalidState("image usage must be nonempty"));
        }
        // SAFETY: the physical device belongs to this live instance; these are
        // core 2D color formats with nonempty usage and no optional create flags.
        let support = unsafe {
            device.instance.get_physical_device_image_format_properties(
                device.physical,
                format,
                vk::ImageType::TYPE_2D,
                vk::ImageTiling::OPTIMAL,
                usage,
                vk::ImageCreateFlags::empty(),
            )
        }
        .map_err(|error| match error {
            vk::Result::ERROR_FORMAT_NOT_SUPPORTED => {
                Error::Unsupported("image usage is not supported by its format")
            }
            error => Error::Vulkan(error),
        })?;
        if width > support.max_extent.width
            || height > support.max_extent.height
            || !support.sample_counts.contains(vk::SampleCountFlags::TYPE_1)
        {
            return Err(Error::InvalidSize);
        }
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: dimensions and color format satisfy the renderer's image
        // boundary. No sparse binding, external memory or optional flags are used.
        let handle = unsafe { device.raw.create_image(&info, None) }.map_err(Error::Vulkan)?;
        let mut image = Self {
            handle,
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::null(),
            allocation: 0,
            device: device.raw.clone(),
        };
        // SAFETY: the image is live and was created from this logical device.
        let requirements = unsafe { device.raw.get_image_memory_requirements(handle) };
        check_budget(requirements.size, budget)?;
        let index = memory_type(
            device,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(handle);
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(index)
            .push_next(&mut dedicated);
        // SAFETY: the memory type, size, and zero offset meet image requirements;
        // the dedicated allocation identifies the sole image bound to it. The
        // wrapper cleans up the image if allocation fails.
        image.memory = unsafe { device.raw.allocate_memory(&info, None) }.map_err(Error::Vulkan)?;
        image.allocation = requirements.size;
        // SAFETY: this allocation is exclusive to the image and is large/aligned.
        unsafe { device.raw.bind_image_memory(handle, image.memory, 0) }.map_err(Error::Vulkan)?;
        let info = vk::ImageViewCreateInfo::default()
            .image(handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        // SAFETY: this bound image has exactly one color mip/layer in the same
        // format. Image outlives the view; failure releases the partial wrapper.
        image.view = unsafe { device.raw.create_image_view(&info, None) }.map_err(Error::Vulkan)?;
        Ok(image)
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: the renderer finished all uses and keeps Device alive. Partial
        // construction has null handles; destroy view before image before memory.
        unsafe {
            if self.view != vk::ImageView::null() {
                self.device.destroy_image_view(self.view, None);
            }
            self.device.destroy_image(self.handle, None);
            if self.memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.memory, None);
            }
        }
    }
}

fn check_budget(required: u64, limit: u64) -> Result {
    if required > limit {
        Err(Error::Budget { required, limit })
    } else {
        Ok(())
    }
}

fn memory_type(device: &Device, bits: u32, properties: vk::MemoryPropertyFlags) -> Result<u32> {
    (0..device.memory.memory_type_count)
        .find(|&index| {
            let flags = device.memory.memory_types[index as usize].property_flags;
            // No protected/coherent-AMD features are enabled, and these resources
            // are not transient attachments eligible for lazily allocated memory.
            bits & (1 << index) != 0
                && flags.contains(properties)
                && !flags.intersects(
                    vk::MemoryPropertyFlags::PROTECTED
                        | vk::MemoryPropertyFlags::DEVICE_COHERENT_AMD
                        | vk::MemoryPropertyFlags::LAZILY_ALLOCATED,
                )
        })
        .ok_or(Error::Unsupported("no compatible Vulkan memory type"))
}
