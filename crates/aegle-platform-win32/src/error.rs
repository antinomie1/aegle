use std::fmt;

/// Native window, input or presentation failure, reported to hosts as the
/// contract's [`aegle_platform::Error`].
#[derive(Debug)]
pub(crate) enum Error {
    /// A native OS operation failed.
    Backend(String),
    /// Zero or unrepresentable extent.
    InvalidSize,
    /// The window is no longer registered with this event loop.
    InvalidWindow,
    /// Title or application identifier contains NUL or exceeds 4000 UTF-8 bytes.
    InvalidString,
    /// The Text Services Framework could not be activated on this thread.
    ImeUnavailable,
    /// Invalid input-method state or candidate geometry.
    InvalidIme(&'static str),
    /// The native input stream contains malformed UTF-16.
    InvalidUtf16,
    /// The requested pixel allocation exceeds the window's bound.
    BufferBudget {
        /// Required allocation in bytes.
        required: usize,
        /// Configured allowance.
        budget: usize,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => f.write_str(error),
            Self::InvalidSize => f.write_str("invalid Windows client size"),
            Self::InvalidWindow => f.write_str("unknown Windows window"),
            Self::InvalidString => {
                f.write_str("window strings must be NUL-free and at most 4000 bytes")
            }
            Self::ImeUnavailable => f.write_str("the Text Services Framework is unavailable"),
            Self::InvalidIme(reason) => write!(f, "invalid IME state: {reason}"),
            Self::InvalidUtf16 => f.write_str("native text contains malformed UTF-16"),
            Self::BufferBudget { required, budget } => write!(
                f,
                "pixel buffer requires {required} bytes, budget is {budget}"
            ),
        }
    }
}
impl std::error::Error for Error {}
impl From<windows::core::Error> for Error {
    fn from(value: windows::core::Error) -> Self {
        Self::Backend(value.to_string())
    }
}

impl From<Error> for aegle_platform::Error {
    fn from(value: Error) -> Self {
        match value {
            Error::Backend(error) => Self::Backend(error),
            Error::InvalidSize => Self::InvalidSize,
            Error::InvalidWindow => Self::InvalidWindow,
            Error::InvalidString => Self::InvalidString,
            Error::ImeUnavailable => Self::ImeUnavailable,
            Error::InvalidIme(reason) => Self::InvalidIme(reason),
            Error::InvalidUtf16 => Self::InvalidText,
            Error::BufferBudget { required, budget } => Self::BufferBudget { required, budget },
        }
    }
}

impl From<aegle_platform::Error> for Error {
    /// Contract validation failures keep their meaning.
    fn from(value: aegle_platform::Error) -> Self {
        match value {
            aegle_platform::Error::InvalidIme(reason) => Self::InvalidIme(reason),
            error => Self::Backend(error.to_string()),
        }
    }
}

/// The byte length of a BGRA8 buffer of `size`, rejecting empty or
/// unrepresentable extents.
pub(crate) fn bytes(size: crate::PixelSize) -> Result<usize, Error> {
    if size.width == 0
        || size.height == 0
        || size.width > i32::MAX as u32 / 4
        || size.height > i32::MAX as u32
    {
        return Err(Error::InvalidSize);
    }
    (size.width as usize)
        .checked_mul(size.height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::InvalidSize)
}
