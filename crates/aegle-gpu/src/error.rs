use std::fmt;

/// Geometry or record-storage failure.
#[derive(Debug)]
pub enum Error {
    /// A mapped coordinate exceeds the finite ±1,048,576 range.
    Coordinates,
    /// More than eight simultaneous clipping scopes, including the external clip.
    ClipDepth,
    /// The recording would exceed its byte limit.
    Budget {
        /// Required bytes under the limit's accounting rules.
        required: u64,
        /// Configured limit in bytes.
        limit: u64,
    },
    /// Host memory allocation failed.
    Allocation,
    /// A transformed scene failed its shared geometry invariant.
    Scene(aegle_scene::SceneError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Coordinates => f.write_str("geometry exceeds the coordinate range"),
            Self::ClipDepth => f.write_str("clip depth exceeds eight layers"),
            Self::Budget { required, limit } => {
                write!(f, "recording requires {required} bytes, limit {limit}")
            }
            Self::Allocation => f.write_str("host allocation failed"),
            Self::Scene(e) => write!(f, "scene: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<aegle_scene::SceneError> for Error {
    fn from(value: aegle_scene::SceneError) -> Self {
        Self::Scene(value)
    }
}

/// Record-building result.
pub type Result<T = ()> = std::result::Result<T, Error>;
