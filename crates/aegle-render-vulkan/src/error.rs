use std::fmt;

/// Vulkan loading, capability, geometry or resource failure.
#[derive(Debug)]
pub enum Error {
    /// The installed Vulkan loader could not be opened.
    Loader(ash::LoadingError),
    /// A Vulkan operation failed, including device loss.
    Vulkan(ash::vk::Result),
    /// The chosen device lacks a required capability.
    Unsupported(&'static str),
    /// The native surface changed; request another frame to recreate its swapchain.
    #[cfg(feature = "window")]
    SurfaceOutOfDate,
    /// Invalid or unsupported target extent.
    InvalidSize,
    /// An operation requires a completed frame or another established state.
    InvalidState(&'static str),
    /// A mapped coordinate exceeds the backend's finite ±1,048,576 range.
    Coordinates,
    /// More than eight simultaneous clipping scopes, including the external clip.
    ClipDepth,
    /// The scene contains a command not implemented by this renderer.
    UnsupportedCommand,
    /// The scene draws a texture that is not registered on this device.
    #[cfg(feature = "text")]
    UnknownTexture,
    /// One submission needs more than its 16 reserved texture bindings. A frame
    /// splits into several submissions instead, so this is returned only when
    /// nothing else was recorded since the last one.
    #[cfg(feature = "text")]
    TooManyTextures,
    /// Glyph data, raster settings or raster-cache resources failed validation.
    #[cfg(feature = "text")]
    Glyph(aegle_glyph::GlyphError),
    /// One submission's glyph working set cannot fit the configured atlas. A
    /// frame splits into several submissions instead, so this is returned only
    /// when a submission holding nothing else still has no room for the entry.
    #[cfg(feature = "text")]
    AtlasFull,
    /// A glyph and its transparent border exceed the configured atlas page size.
    #[cfg(feature = "text")]
    GlyphTooLarge,
    /// The configured text resource limits cannot be used by this renderer.
    #[cfg(feature = "text")]
    InvalidTextOptions,
    /// A transformed scene failed its shared geometry invariant.
    Scene(aegle_scene::SceneError),
    /// An explicit allocation budget was exceeded.
    Budget {
        /// Required bytes under the corresponding budget's accounting rules.
        required: u64,
        /// Configured limit in bytes.
        limit: u64,
    },
    /// Host memory allocation failed.
    Allocation,
    /// An earlier draw failed; this frame cannot be submitted.
    FrameFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loader(e) => write!(f, "Vulkan loader: {e}"),
            Self::Vulkan(e) => write!(f, "Vulkan: {e:?}"),
            Self::Unsupported(capability) => {
                write!(f, "Vulkan capability unavailable: {capability}")
            }
            #[cfg(feature = "window")]
            Self::SurfaceOutOfDate => f.write_str("Vulkan surface changed; redraw required"),
            Self::InvalidSize => f.write_str("invalid Vulkan render target size"),
            Self::InvalidState(reason) => f.write_str(reason),
            Self::Coordinates => f.write_str("Vulkan geometry exceeds the coordinate range"),
            Self::ClipDepth => f.write_str("Vulkan clip depth exceeds eight layers"),
            Self::UnsupportedCommand => f.write_str("unsupported Vulkan scene command"),
            #[cfg(feature = "text")]
            Self::UnknownTexture => f.write_str("scene texture is not registered with this device"),
            #[cfg(feature = "text")]
            Self::TooManyTextures => {
                f.write_str("a submission draws more than 16 registered textures")
            }
            #[cfg(feature = "text")]
            Self::Glyph(e) => write!(f, "Vulkan glyph: {e}"),
            #[cfg(feature = "text")]
            Self::AtlasFull => f.write_str("Vulkan atlas has no room for one more entry"),
            #[cfg(feature = "text")]
            Self::GlyphTooLarge => f.write_str("Vulkan glyph exceeds atlas page dimensions"),
            #[cfg(feature = "text")]
            Self::InvalidTextOptions => f.write_str("invalid Vulkan text resource limits"),
            Self::Scene(e) => write!(f, "Vulkan scene: {e}"),
            Self::Budget { required, limit } => write!(
                f,
                "Vulkan allocation requires {required} bytes, limit {limit}"
            ),
            Self::Allocation => f.write_str("Vulkan host allocation failed"),
            Self::FrameFailed => f.write_str("cannot submit a failed Vulkan frame"),
        }
    }
}
impl std::error::Error for Error {}
impl From<ash::LoadingError> for Error {
    fn from(value: ash::LoadingError) -> Self {
        Self::Loader(value)
    }
}
impl From<ash::vk::Result> for Error {
    fn from(value: ash::vk::Result) -> Self {
        Self::Vulkan(value)
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

/// Vulkan renderer operation result.
pub type Result<T = ()> = std::result::Result<T, Error>;
