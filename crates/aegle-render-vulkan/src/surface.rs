//! Native surface creation and device suitability, limited to actual backends.
#![allow(unsafe_code)]
use crate::{Error, Result};
use ash::{Entry, vk};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

pub(crate) struct Surface {
    pub loader: ash::khr::surface::Instance,
    pub handle: vk::SurfaceKHR,
}

pub(crate) fn extensions(display: RawDisplayHandle) -> Result<[*const i8; 2]> {
    let platform = match display {
        RawDisplayHandle::Wayland(_) => ash::khr::wayland_surface::NAME,
        RawDisplayHandle::Windows(_) => ash::khr::win32_surface::NAME,
        _ => {
            return Err(Error::Unsupported(
                "only Wayland and Win32 surfaces are supported",
            ));
        }
    };
    Ok([ash::khr::surface::NAME.as_ptr(), platform.as_ptr()])
}
impl Surface {
    // Caller keeps the matching native window and display owners alive until Drop.
    pub fn new(
        entry: &Entry,
        instance: &ash::Instance,
        display: RawDisplayHandle,
        window: RawWindowHandle,
    ) -> Result<Self> {
        // SAFETY: WindowRenderer retains the owners supplying these borrowed handles.
        // The instance enabled the matching platform extension before this call.
        let handle = unsafe {
            match (display, window) {
                (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(w)) => {
                    ash::khr::wayland_surface::Instance::new(entry, instance)
                        .create_wayland_surface(
                            &vk::WaylandSurfaceCreateInfoKHR::default()
                                .display(d.display.as_ptr().cast())
                                .surface(w.surface.as_ptr().cast()),
                            None,
                        )?
                }
                (RawDisplayHandle::Windows(_), RawWindowHandle::Win32(w)) => {
                    let instance_handle = w
                        .hinstance
                        .ok_or(Error::Unsupported("Win32 HINSTANCE is required"))?;
                    ash::khr::win32_surface::Instance::new(entry, instance).create_win32_surface(
                        &vk::Win32SurfaceCreateInfoKHR::default()
                            .hinstance(instance_handle.get())
                            .hwnd(w.hwnd.get()),
                        None,
                    )?
                }
                _ => {
                    return Err(Error::Unsupported(
                        "native display/window handles do not match",
                    ));
                }
            }
        };
        Ok(Self {
            loader: ash::khr::surface::Instance::new(entry, instance),
            handle,
        })
    }
    pub fn format(&self, physical: vk::PhysicalDevice) -> Result<vk::SurfaceFormatKHR> {
        // SAFETY: The surface and physical device belong to the same live instance.
        let formats = unsafe {
            self.loader
                .get_physical_device_surface_formats(physical, self.handle)?
        };
        for format in [vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM] {
            if let Some(choice) = formats
                .iter()
                .find(|f| f.format == format && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
            {
                return Ok(*choice);
            }
        }
        if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
            return Ok(vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_UNORM,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            });
        }
        Err(Error::Unsupported(
            "surface needs RGBA8/BGRA8 UNORM in sRGB color space",
        ))
    }
    pub fn supports(
        &self,
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        family: u32,
    ) -> Result {
        // SAFETY: All queried handles belong to this live instance.
        unsafe {
            if !self
                .loader
                .get_physical_device_surface_support(physical, family, self.handle)?
            {
                return Err(Error::Unsupported(
                    "graphics queue cannot present to this surface",
                ));
            }
            let extensions = instance.enumerate_device_extension_properties(physical)?;
            if !extensions.iter().any(|e| {
                std::ffi::CStr::from_ptr(e.extension_name.as_ptr()) == ash::khr::swapchain::NAME
            }) {
                return Err(Error::Unsupported("VK_KHR_swapchain is required"));
            }
        }
        let _ = self.format(physical)?;
        Ok(())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: Device waits and destroys all swapchains before dropping surface;
        // WindowRenderer keeps the native owners alive beyond the renderer.
        unsafe {
            self.loader.destroy_surface(self.handle, None);
        }
    }
}
