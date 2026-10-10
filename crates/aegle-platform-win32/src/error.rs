use std::fmt;

/// Native window, input or presentation failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
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
#[cfg(windows)]
impl From<windows::core::Error> for Error {
    fn from(value: windows::core::Error) -> Self {
        Self::Backend(value.to_string())
    }
}

/// Incomplete frames are never sent to GDI.
#[derive(Debug)]
#[non_exhaustive]
pub enum PresentError<E> {
    /// Native or allocation failure.
    Platform(Error),
    /// Application drawing error.
    Draw(E),
}
impl<E: fmt::Display> fmt::Display for PresentError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Platform(e) => e.fmt(f),
            Self::Draw(e) => e.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PresentError<E> {}
