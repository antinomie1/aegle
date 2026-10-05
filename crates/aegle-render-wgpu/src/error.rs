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
    /// Glyph data or raster settings failed validation.
    #[cfg(feature = "text")]
    Glyph(aegle_glyph::GlyphError),
    /// A glyph exceeds the atlas page, or an image or path mask exceeds the
    /// device's texture size.
    #[cfg(feature = "text")]
    TooLarge,
    /// A transformed scene failed its shared geometry invariant.
    Scene(aegle_scene::SceneError),
    /// Copying pixels back to the CPU failed.
    Readback(wgpu::BufferAsyncError),
    /// An earlier draw failed; this frame cannot be submitted.
    FrameFailed,
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
            Self::UnsupportedCommand => f.write_str("unsupported wgpu scene command"),
            #[cfg(feature = "text")]
            Self::Glyph(e) => write!(f, "wgpu glyph: {e}"),
            #[cfg(feature = "text")]
            Self::TooLarge => f.write_str("wgpu glyph, image or path mask exceeds texture limits"),
            Self::Scene(e) => write!(f, "wgpu scene: {e}"),
            Self::Readback(e) => write!(f, "wgpu readback: {e}"),
            Self::FrameFailed => f.write_str("cannot submit a failed wgpu frame"),
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
