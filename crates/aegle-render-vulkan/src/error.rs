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
    /// Glyph data, raster settings or raster-cache resources failed validation.
    #[cfg(feature = "text")]
    Glyph(aegle_glyph::GlyphError),
    /// The current frame's glyph working set cannot fit the configured atlas.
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
            Self::Glyph(e) => write!(f, "Vulkan glyph: {e}"),
            #[cfg(feature = "text")]
            Self::AtlasFull => f.write_str("Vulkan glyph working set exceeds atlas capacity"),
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
