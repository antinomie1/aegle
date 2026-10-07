use std::fmt;

/// Adapter, device, geometry or resource failure.
#[derive(Debug)]
pub enum Error {
    /// No adapter satisfies the request (for the window variant, the surface).
    Adapter(wgpu::RequestAdapterError),
    /// The adapter refused the device request.
    Device(wgpu::RequestDeviceError),
    /// The native surface could not be created.
    #[cfg(feature = "window")]
    Surface(wgpu::CreateSurfaceError),
    /// The chosen device or surface lacks a required capability.
    Unsupported(&'static str),
    /// The surface changed; request another frame to reconfigure it.
    #[cfg(feature = "window")]
    SurfaceOutOfDate,
    /// The surface was lost; create a new window renderer.
    #[cfg(feature = "window")]
    SurfaceLost,
    /// Invalid or unsupported target extent.
    InvalidSize,
    /// An operation requires a completed frame or another established state.
    InvalidState(&'static str),
    /// A mapped coordinate exceeds the backend's finite ±1,048,576 range.
    Coordinates,
    /// More than eight simultaneous clipping scopes, including the external clip.
    ClipDepth,
    /// The scene contains a command this renderer does not implement.
    UnsupportedCommand,
    /// The scene draws a texture that is not registered on this device.
    UnknownTexture,
    /// Glyph data or raster settings failed validation.
    #[cfg(feature = "text")]
    Glyph(aegle_glyph::GlyphError),
    /// A glyph exceeds the atlas page, or an image or path mask exceeds the
    /// device's texture size.
    #[cfg(feature = "text")]
    TooLarge,
    /// A transformed scene failed its shared geometry invariant.
    Scene(aegle_scene::SceneError),
    /// Host memory allocation failed.
    Allocation,
    /// Copying pixels back to the CPU failed.
    Readback(wgpu::BufferAsyncError),
    /// An earlier draw failed; this frame cannot be submitted.
    FrameFailed,
    /// wgpu reported a validation, out-of-memory or internal error. The device
    /// state is unknown, so this renderer accepts no more work.
    Gpu(String),
    /// The device was lost. This renderer accepts no more work; Aegle does not
    /// recreate devices.
    DeviceLost(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(e) => write!(f, "wgpu adapter: {e}"),
            Self::Device(e) => write!(f, "wgpu device: {e}"),
            #[cfg(feature = "window")]
            Self::Surface(e) => write!(f, "wgpu surface: {e}"),
            Self::Unsupported(capability) => {
                write!(f, "wgpu capability unavailable: {capability}")
            }
            #[cfg(feature = "window")]
            Self::SurfaceOutOfDate => f.write_str("wgpu surface changed; redraw required"),
            #[cfg(feature = "window")]
            Self::SurfaceLost => f.write_str("wgpu surface lost"),
            Self::InvalidSize => f.write_str("invalid wgpu render target size"),
            Self::InvalidState(reason) => f.write_str(reason),
            Self::Coordinates => f.write_str("wgpu geometry exceeds the coordinate range"),
            Self::ClipDepth => f.write_str("wgpu clip depth exceeds eight layers"),
            Self::UnknownTexture => {
                f.write_str("scene texture is not registered with this renderer")
            }
            Self::UnsupportedCommand => f.write_str("unsupported wgpu scene command"),
            #[cfg(feature = "text")]
            Self::Glyph(e) => write!(f, "wgpu glyph: {e}"),
            #[cfg(feature = "text")]
            Self::TooLarge => f.write_str("wgpu glyph, image or path mask exceeds texture limits"),
            Self::Scene(e) => write!(f, "wgpu scene: {e}"),
            Self::Readback(e) => write!(f, "wgpu readback: {e}"),
            Self::Allocation => f.write_str("wgpu host allocation failed"),
            Self::FrameFailed => f.write_str("cannot submit a failed wgpu frame"),
            Self::Gpu(message) => write!(f, "wgpu error: {message}"),
            Self::DeviceLost(message) => write!(f, "wgpu device lost: {message}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<wgpu::RequestAdapterError> for Error {
    fn from(value: wgpu::RequestAdapterError) -> Self {
        Self::Adapter(value)
    }
}
impl From<wgpu::RequestDeviceError> for Error {
    fn from(value: wgpu::RequestDeviceError) -> Self {
        Self::Device(value)
    }
}
impl From<aegle_gpu::Error> for Error {
    fn from(value: aegle_gpu::Error) -> Self {
        match value {
            aegle_gpu::Error::Coordinates => Self::Coordinates,
            aegle_gpu::Error::ClipDepth => Self::ClipDepth,
            aegle_gpu::Error::Allocation => Self::Allocation,
            aegle_gpu::Error::Scene(error) => Self::Scene(error),
        }
    }
}
impl From<aegle_scene::SceneError> for Error {
    fn from(value: aegle_scene::SceneError) -> Self {
        Self::Scene(value)
    }
}
#[cfg(feature = "text")]
impl From<aegle_glyph::GlyphError> for Error {
    fn from(value: aegle_glyph::GlyphError) -> Self {
        Self::Glyph(value)
    }
}

/// Renderer operation result.
pub type Result<T = ()> = std::result::Result<T, Error>;
