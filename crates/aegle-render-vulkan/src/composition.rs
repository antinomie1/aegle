//! Transparent Win32 presentation through DirectComposition.
//!
//! Vulkan HWND swapchains offer only opaque composition on Windows, so a
//! transparent window is created without a redirection surface and shows a
//! premultiplied DXGI composition swapchain instead. Vulkan draws the frame
//! into a D3D11 texture it imports; each present copies that texture into the
//! swapchain's back buffer on a D3D11 device of the same adapter.
#![allow(unsafe_code)]

use std::{
    cell::{Cell, RefCell},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    rc::Rc,
};

use ash::vk;
use windows::{
    Win32::{
        Foundation::{HANDLE, HMODULE, HWND, LUID},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_FENCE_FLAG_NONE,
                D3D11_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice,
                ID3D11Device, ID3D11Device5, ID3D11DeviceContext, ID3D11DeviceContext4,
                ID3D11Fence, ID3D11Texture2D,
            },
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
                IDCompositionVisual,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT,
                DXGI_SCALING_STRETCH, DXGI_SHARED_RESOURCE_READ, DXGI_SHARED_RESOURCE_WRITE,
                DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIDevice, IDXGIFactory4,
                IDXGIResource1, IDXGISwapChain1,
            },
        },
        System::Threading::{CreateEventW, INFINITE, WaitForSingleObject},
    },
    core::{Interface, PCWSTR},
};

use crate::{Error, Result, device::Device};

const HANDLE_TYPE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::D3D11_TEXTURE;

/// A window's D3D11 device and DirectComposition tree, kept across resizes.
pub(crate) struct Compositor {
    device: ID3D11Device,
    context: ID3D11DeviceContext4,
    /// Signalled after each copy, so Vulkan never draws into a texture D3D11
    /// is still reading.
    fence: ID3D11Fence,
    copies: Cell<u64>,
    event: OwnedHandle,
    factory: IDXGIFactory4,
    /// Created by the first frame, then resized with the window.
    chain: RefCell<Option<IDXGISwapChain1>>,
    dcomp: IDCompositionDevice,
    // Shows `visual` on the window for as long as the compositor lives.
    _target: IDCompositionTarget,
    visual: IDCompositionVisual,
}

impl Compositor {
    pub fn new(device: &Device, hwnd: vk::HWND) -> Result<Self> {
        if !device.external_memory_win32 {
            return Err(Error::Unsupported(
                "a transparent Win32 window needs VK_KHR_external_memory_win32",
            ));
        }
        let mut id = vk::PhysicalDeviceIDProperties::default();
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
        // SAFETY: the physical device belongs to this live Vulkan 1.1 instance.
        unsafe {
            device
                .instance
                .get_physical_device_properties2(device.physical, &mut properties)
        };
        if id.device_luid_valid == vk::FALSE {
            return Err(Error::Unsupported("the Vulkan device reports no adapter LUID"));
        }
        let luid = LUID {
            LowPart: u32::from_ne_bytes(id.device_luid[..4].try_into().unwrap()),
            HighPart: i32::from_ne_bytes(id.device_luid[4..].try_into().unwrap()),
        };
        // SAFETY: plain COM creation on the adapter Vulkan drives; every out
        // pointer is a live local and the HWND is owned by the window renderer.
        unsafe {
            let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let adapter: IDXGIAdapter = factory.EnumAdapterByLuid(luid)?;
            let (mut d3d, mut context) = (None, None);
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                Some(&mut context),
            )?;
            let (d3d, context): (ID3D11Device, ID3D11DeviceContext) =
                (d3d.unwrap(), context.unwrap());
            let mut fence: Option<ID3D11Fence> = None;
            d3d.cast::<ID3D11Device5>()?
                .CreateFence(0, D3D11_FENCE_FLAG_NONE, &mut fence)?;
            let event = OwnedHandle::from_raw_handle(CreateEventW(None, false, false, None)?.0);
            let dcomp: IDCompositionDevice =
                DCompositionCreateDevice(&d3d.cast::<IDXGIDevice>()?)?;
            let target = dcomp.CreateTargetForHwnd(HWND(hwnd as _), true)?;
            let visual = dcomp.CreateVisual()?;
            target.SetRoot(&visual)?;
            Ok(Self {
                device: d3d,
                context: context.cast()?,
                fence: fence.unwrap(),
                copies: Cell::new(0),
                event,
                factory,
                chain: RefCell::new(None),
                dcomp,
                _target: target,
                visual,
            })
        }
    }

    /// Sizes the swapchain for the next frames, creating it the first time.
    fn resize(&self, format: DXGI_FORMAT, extent: vk::Extent2D) -> Result {
        let mut chain = self.chain.borrow_mut();
        // SAFETY: no back buffer reference outlives `present`, as resizing needs.
        unsafe {
            if let Some(chain) = chain.as_ref() {
                return Ok(chain.ResizeBuffers(
                    0,
                    extent.width,
                    extent.height,
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )?);
            }
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: extent.width,
                Height: extent.height,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let created = self
                .factory
                .CreateSwapChainForComposition(&self.device, &desc, None)?;
            self.visual.SetContent(&created)?;
            self.dcomp.Commit()?;
            *chain = Some(created);
        }
        Ok(())
    }
}

/// The image Vulkan draws a transparent window's frames into: a D3D11 texture
/// imported as dedicated memory, copied to the composition swapchain.
pub(crate) struct Shared {
    compositor: Rc<Compositor>,
    texture: ID3D11Texture2D,
    raw: ash::Device,
    pub image: vk::Image,
    memory: vk::DeviceMemory,
}

impl Shared {
    pub fn new(
        device: &Device,
        compositor: Rc<Compositor>,
        format: vk::Format,
        extent: vk::Extent2D,
        usage: vk::ImageUsageFlags,
    ) -> Result<Self> {
        let dxgi = match format {
            vk::Format::B8G8R8A8_UNORM => DXGI_FORMAT_B8G8R8A8_UNORM,
            vk::Format::R8G8B8A8_UNORM => DXGI_FORMAT_R8G8B8A8_UNORM,
            _ => return Err(Error::Unsupported("composition needs an RGBA8 UNORM format")),
        };
        compositor.resize(dxgi, extent)?;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: extent.width,
            Height: extent.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: dxgi,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0)
                as u32,
        };
        // SAFETY: the description is complete and the out pointer is a local;
        // the NT handle is owned here and closed after Vulkan imports it.
        let (texture, handle) = unsafe {
            let mut texture = None;
            compositor
                .device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture: ID3D11Texture2D = texture.unwrap();
            let handle = texture.cast::<IDXGIResource1>()?.CreateSharedHandle(
                None,
                (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0,
                PCWSTR::null(),
            )?;
            (texture, OwnedHandle::from_raw_handle(handle.0))
        };
        let mut this = Self {
            compositor,
            texture,
            raw: device.raw.clone(),
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
        };
        let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(HANDLE_TYPE);
        let info = vk::ImageCreateInfo::default()
            .push_next(&mut external)
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let loader = ash::khr::external_memory_win32::Device::new(&device.instance, &device.raw);
        let handle = handle.as_raw_handle() as vk::HANDLE;
        // SAFETY: the device enabled VK_KHR_external_memory_win32; the image is
        // created for, and bound only to, the imported texture. Importing an NT
        // handle does not take ownership of it, so `handle` closes on return.
        unsafe {
            this.image = device.raw.create_image(&info, None)?;
            let requirements = device.raw.get_image_memory_requirements(this.image);
            let mut properties = vk::MemoryWin32HandlePropertiesKHR::default();
            loader.get_memory_win32_handle_properties(HANDLE_TYPE, handle, &mut properties)?;
            let kind = crate::memory::memory_type(
                device,
                requirements.memory_type_bits & properties.memory_type_bits,
                vk::MemoryPropertyFlags::empty(),
            )?;
            let mut import = vk::ImportMemoryWin32HandleInfoKHR::default()
                .handle_type(HANDLE_TYPE)
                .handle(handle);
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(this.image);
            this.memory = device.raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(kind)
                    .push_next(&mut import)
                    .push_next(&mut dedicated),
                None,
            )?;
            device.raw.bind_image_memory(this.image, this.memory, 0)?;
        }
        Ok(this)
    }

    /// Waits until D3D11 finished copying the previous frame out of the image.
    pub fn acquire(&self) -> Result {
        let compositor = &self.compositor;
        let copied = compositor.copies.get();
        // SAFETY: the fence and the event are owned by the live compositor.
        unsafe {
            if compositor.fence.GetCompletedValue() < copied {
                compositor
                    .fence
                    .SetEventOnCompletion(copied, HANDLE(compositor.event.as_raw_handle()))?;
                WaitForSingleObject(HANDLE(compositor.event.as_raw_handle()), INFINITE);
            }
        }
        Ok(())
    }

    /// Copies a finished frame to the swapchain and presents it at the next
    /// vertical blank. The caller has waited for the Vulkan work drawing it.
    pub fn present(&self) -> Result {
        let compositor = &self.compositor;
        let chain = compositor.chain.borrow();
        let chain = chain.as_ref().expect("sized when the image was created");
        let copied = compositor.copies.get() + 1;
        // SAFETY: both textures are live and the same size and format; the
        // back buffer reference ends before any later resize.
        unsafe {
            let back: ID3D11Texture2D = chain.GetBuffer(0)?;
            compositor.context.CopyResource(&back, &self.texture);
            compositor.context.Signal(&compositor.fence, copied)?;
            chain.Present(1, DXGI_PRESENT(0)).ok()?;
        }
        compositor.copies.set(copied);
        Ok(())
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // SAFETY: the renderer waits for the device before dropping a
        // swapchain; null handles from a partial construction are ignored.
        unsafe {
            self.raw.destroy_image(self.image, None);
            self.raw.free_memory(self.memory, None);
        }
    }
}
