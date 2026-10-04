use std::fmt;

/// Window, input or software storage failure.
#[derive(Debug)]
pub enum Error {
    /// A required protocol, connection or OS operation failed.
    Backend(String),
    /// An extent is empty or cannot be represented by the protocol.
    InvalidSize,
    /// A window no longer belongs to this connection.
    InvalidWindow,
    /// A window string contains NUL or exceeds the 4000-byte protocol allowance.
    InvalidString,
    /// The compositor does not advertise text-input-v3.
    ImeUnavailable,
    /// Invalid surrounding text, selection or candidate geometry.
    InvalidIme(&'static str),
    /// Live old and new SHM mappings would exceed the window allowance.
    BufferBudget {
        /// Required mapping bytes, including slot alignment.
        required: usize,
        /// Configured mapping allowance.
        budget: usize,
    },
}

impl Error {
    pub(crate) fn backend(error: impl fmt::Display) -> Self {
        Self::Backend(error.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => f.write_str(error),
            Self::InvalidSize => f.write_str("invalid Wayland surface size"),
            Self::InvalidWindow => f.write_str("unknown Wayland window"),
            Self::InvalidString => {
                f.write_str("window strings must be NUL-free and at most 4000 UTF-8 bytes")
            }
            Self::ImeUnavailable => f.write_str("text-input-v3 is unavailable"),
            Self::InvalidIme(reason) => write!(f, "invalid IME state: {reason}"),
            Self::BufferBudget { required, budget } => {
                write!(
                    f,
                    "SHM mappings require {required} bytes, budget is {budget}"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// Presentation preserves the application's original drawing error.
#[derive(Debug)]
pub enum PresentError<E> {
    /// Window or SHM failure.
    Platform(Error),
    /// Drawing failed; the incomplete frame was not attached.
    Draw(E),
}

impl<E: fmt::Display> fmt::Display for PresentError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Platform(error) => error.fmt(f),
            Self::Draw(error) => error.fmt(f),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for PresentError<E> {}
